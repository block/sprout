import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:image_picker/image_picker.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:pointycastle/digests/sha256.dart';

import 'relay_provider.dart';

const _mediaUploadPath = '/media/upload';
const _mediaUploadPlatformChannelName = 'sprout/media_upload';
const _sanitizeImageForUploadMethod = 'sanitizeImageForUpload';
const _transcodeImageToJpegMethod = 'transcodeImageToJpeg';
const _uploadAuthKind = 24242;
const _uploadAuthLifetimeSeconds = 300;
const _heicBrands = {
  'heic',
  'heix',
  'hevc',
  'hevx',
  'heim',
  'heis',
  'mif1',
  'msf1',
};
final _mediaUploadPlatformChannel = MethodChannel(
  _mediaUploadPlatformChannelName,
);

const _allowedImageMimeTypes = {'image/jpeg', 'image/png', 'image/webp'};
const _unsupportedAnimatedImageMimeTypes = {'image/gif'};
const _unsupportedGifUploadMessage =
    'GIF uploads are not supported on mobile yet';
const _unsupportedAnimatedPngUploadMessage =
    'Animated PNG uploads are not supported on mobile yet';
const _unsupportedAnimatedWebpUploadMessage =
    'Animated WebP uploads are not supported on mobile yet';

typedef PickGalleryImage = Future<XFile?> Function();
typedef SanitizeImageBytes =
    Future<Uint8List> Function(Uint8List bytes, String mimeType);
typedef TranscodeImageToJpeg = Future<Uint8List> Function(Uint8List bytes);

@immutable
class _PreparedUploadImage {
  final Uint8List bytes;
  final String mimeType;

  const _PreparedUploadImage({required this.bytes, required this.mimeType});
}

@immutable
class BlobDescriptor {
  final String url;
  final String sha256;
  final int size;
  final String type;
  final int uploaded;
  final String? dim;
  final String? blurhash;
  final String? thumb;
  final double? duration;
  final String? image;

  const BlobDescriptor({
    required this.url,
    required this.sha256,
    required this.size,
    required this.type,
    required this.uploaded,
    this.dim,
    this.blurhash,
    this.thumb,
    this.duration,
    this.image,
  });

  factory BlobDescriptor.fromJson(Map<String, dynamic> json) => BlobDescriptor(
    url: json['url'] as String,
    sha256: json['sha256'] as String,
    size: (json['size'] as num).toInt(),
    type: json['type'] as String,
    uploaded: (json['uploaded'] as num).toInt(),
    dim: json['dim'] as String?,
    blurhash: json['blurhash'] as String?,
    thumb: json['thumb'] as String?,
    duration: (json['duration'] as num?)?.toDouble(),
    image: json['image'] as String?,
  );

  List<String> toImetaTag() => [
    'imeta',
    'url $url',
    'm $type',
    'x $sha256',
    'size $size',
    if (dim != null) 'dim $dim',
    if (blurhash != null) 'blurhash $blurhash',
    if (thumb != null) 'thumb $thumb',
    if (duration != null) 'duration $duration',
    if (image != null) 'image $image',
  ];

  String toMarkdownImage() =>
      type.startsWith('video/') ? '![video]($url)' : '![image]($url)';
}

class MediaUploadService {
  final String _baseUrl;
  final String? _apiToken;
  final String? _nsec;
  final PickGalleryImage _pickGalleryImage;
  final SanitizeImageBytes _sanitizeImageBytes;
  final TranscodeImageToJpeg _transcodeImageToJpeg;
  final DateTime Function() _now;
  final http.Client _http;
  final bool _ownsHttpClient;

  MediaUploadService({
    required String baseUrl,
    required String? apiToken,
    required String? nsec,
    required PickGalleryImage pickGalleryImage,
    SanitizeImageBytes? sanitizeImageBytes,
    TranscodeImageToJpeg? transcodeImageToJpeg,
    DateTime Function()? now,
    http.Client? httpClient,
  }) : _baseUrl = baseUrl,
       _apiToken = apiToken,
       _nsec = nsec,
       _pickGalleryImage = pickGalleryImage,
       _sanitizeImageBytes = sanitizeImageBytes ?? _sanitizePickedImageBytes,
       _transcodeImageToJpeg =
           transcodeImageToJpeg ?? _transcodePickedImageToJpeg,
       _now = now ?? DateTime.now,
       _http = httpClient ?? http.Client(),
       _ownsHttpClient = httpClient == null;

  void dispose() {
    if (_ownsHttpClient) {
      _http.close();
    }
  }

  Future<BlobDescriptor?> pickAndUploadImage() async {
    final pickedImage = await _pickGalleryImage();
    if (pickedImage == null) return null;
    final preparedImage = await _prepareUploadImage(pickedImage);
    return uploadBytes(preparedImage.bytes, mimeType: preparedImage.mimeType);
  }

  Future<BlobDescriptor> uploadBytes(
    Uint8List bytes, {
    required String mimeType,
  }) async {
    _validateUpload(bytes, mimeType);
    if (!_allowedImageMimeTypes.contains(mimeType)) {
      throw Exception('unsupported file type: $mimeType');
    }

    final sha256 = _sha256Hex(bytes);
    final request = _buildUploadRequest(
      bytes: bytes,
      mimeType: mimeType,
      sha256: sha256,
    );

    final streamed = await _http.send(request);
    final response = await http.Response.fromStream(streamed);
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw Exception(
        'upload failed (${response.statusCode}): ${response.body}',
      );
    }

    return BlobDescriptor.fromJson(
      jsonDecode(response.body) as Map<String, dynamic>,
    );
  }

  http.Request _buildUploadRequest({
    required Uint8List bytes,
    required String mimeType,
    required String sha256,
  }) {
    final request = http.Request(
      'PUT',
      Uri.parse(_baseUrl).resolve(_mediaUploadPath),
    );
    request.bodyBytes = bytes;
    request.headers.addAll(
      _buildUploadHeaders(mimeType: mimeType, sha256: sha256),
    );
    return request;
  }

  Map<String, String> _buildUploadHeaders({
    required String mimeType,
    required String sha256,
  }) {
    final headers = <String, String>{
      'Authorization': _buildUploadAuthHeader(sha256),
      'Content-Type': mimeType,
      'X-SHA-256': sha256,
    };
    if (_apiToken case final token? when token.isNotEmpty) {
      headers['X-Auth-Token'] = token;
    }
    return headers;
  }

  String _buildUploadAuthHeader(String sha256) {
    final authEvent = _buildUploadAuthEvent(sha256);
    final authJson = jsonEncode(authEvent.toJson());
    final encoded = base64Url.encode(utf8.encode(authJson)).replaceAll('=', '');
    return 'Nostr $encoded';
  }

  nostr.Event _buildUploadAuthEvent(String sha256) {
    final nsec = _nsec;
    if (nsec == null || nsec.isEmpty) {
      throw Exception('Cannot upload media: no signing key available');
    }

    final privkeyHex = nostr.Nip19.decodePrivkey(nsec);
    if (privkeyHex.isEmpty) {
      throw Exception('Invalid nsec');
    }

    final expiration =
        (_now().millisecondsSinceEpoch ~/ 1000) + _uploadAuthLifetimeSeconds;
    final tags = <List<String>>[
      ['t', 'upload'],
      ['x', sha256],
      ['expiration', '$expiration'],
      if (_extractServerAuthority(_baseUrl) case final authority?)
        ['server', authority],
    ];

    return nostr.Event.from(
      kind: _uploadAuthKind,
      content: 'Upload sprout-media',
      tags: tags,
      privkey: privkeyHex,
      verify: false,
    );
  }

  Future<_PreparedUploadImage> _prepareUploadImage(XFile pickedImage) async {
    final bytes = await pickedImage.readAsBytes();
    final detectedMimeType = _tryDetectImageMimeType(bytes);
    if (detectedMimeType case final mimeType?) {
      return _prepareDetectedUploadImage(bytes, mimeType);
    }

    if (_shouldTranscodePickedImage(pickedImage, bytes)) {
      return _prepareTranscodedUploadImage(bytes);
    }

    throw Exception('unsupported file type');
  }

  Future<_PreparedUploadImage> _prepareDetectedUploadImage(
    Uint8List bytes,
    String mimeType,
  ) async {
    _validateUpload(bytes, mimeType);
    final preparedBytes = await _sanitizeImageBytesIfNeeded(bytes, mimeType);
    return _buildPreparedUploadImage(preparedBytes);
  }

  Future<_PreparedUploadImage> _prepareTranscodedUploadImage(
    Uint8List bytes,
  ) async {
    final transcodedBytes = await _transcodeImageToJpeg(bytes);
    return _buildPreparedUploadImage(transcodedBytes);
  }

  _PreparedUploadImage _buildPreparedUploadImage(Uint8List bytes) {
    return _PreparedUploadImage(
      bytes: bytes,
      mimeType: _detectImageMimeType(bytes),
    );
  }

  Future<Uint8List> _sanitizeImageBytesIfNeeded(
    Uint8List bytes,
    String mimeType,
  ) async {
    if (!_shouldSanitizePickedImage(mimeType)) {
      return bytes;
    }

    final sanitizedBytes = await _sanitizeImageBytes(bytes, mimeType);
    if (sanitizedBytes.isEmpty) {
      throw Exception('failed to sanitize image for upload');
    }
    return sanitizedBytes;
  }
}

String _sha256Hex(Uint8List bytes) {
  final digest = SHA256Digest().process(bytes);
  return digest.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
}

String? _tryDetectImageMimeType(Uint8List bytes) {
  try {
    return _detectImageMimeType(bytes);
  } on Exception {
    return null;
  }
}

void _validateUpload(Uint8List bytes, String mimeType) {
  if (_unsupportedAnimatedImageMimeTypes.contains(mimeType)) {
    throw Exception(_unsupportedGifUploadMessage);
  }
  if (mimeType == 'image/png' && _isAnimatedPng(bytes)) {
    throw Exception(_unsupportedAnimatedPngUploadMessage);
  }
  if (mimeType == 'image/webp' && _isAnimatedWebp(bytes)) {
    throw Exception(_unsupportedAnimatedWebpUploadMessage);
  }
}

String _detectImageMimeType(Uint8List bytes) {
  if (_startsWith(bytes, const [0xff, 0xd8, 0xff])) {
    return 'image/jpeg';
  }
  if (_startsWith(bytes, const [
    0x89,
    0x50,
    0x4e,
    0x47,
    0x0d,
    0x0a,
    0x1a,
    0x0a,
  ])) {
    return 'image/png';
  }
  if (_startsWith(bytes, ascii.encode('GIF87a')) ||
      _startsWith(bytes, ascii.encode('GIF89a'))) {
    return 'image/gif';
  }
  if (_startsWith(bytes, ascii.encode('RIFF')) &&
      bytes.length >= 12 &&
      ascii.decode(bytes.sublist(8, 12), allowInvalid: true) == 'WEBP') {
    return 'image/webp';
  }
  throw Exception('unsupported file type');
}

bool _shouldTranscodePickedImage(XFile pickedImage, Uint8List bytes) {
  return _supportsNativeUploadImageProcessing() &&
      (_hasHeicFileExtension(pickedImage) || _looksLikeHeicOrHeif(bytes));
}

bool _isAnimatedPng(Uint8List bytes) {
  if (!_startsWith(bytes, const [
    0x89,
    0x50,
    0x4e,
    0x47,
    0x0d,
    0x0a,
    0x1a,
    0x0a,
  ])) {
    return false;
  }

  var offset = 8;
  while (offset + 12 <= bytes.length) {
    final chunkSize = _readUint32BigEndian(bytes, offset);
    if (offset + 12 + chunkSize > bytes.length) {
      return false;
    }

    if (_matchesAscii(bytes, offset + 4, 'acTL')) {
      return true;
    }

    offset += 12 + chunkSize;
  }

  return false;
}

bool _isAnimatedWebp(Uint8List bytes) {
  if (!_startsWith(bytes, ascii.encode('RIFF')) ||
      bytes.length < 12 ||
      ascii.decode(bytes.sublist(8, 12), allowInvalid: true) != 'WEBP') {
    return false;
  }

  var offset = 12;
  while (offset + 8 <= bytes.length) {
    final chunkSize = _readUint32LittleEndian(bytes, offset + 4);
    final payloadOffset = offset + 8;
    if (payloadOffset + chunkSize > bytes.length) {
      return false;
    }

    if (_matchesAscii(bytes, offset, 'ANIM') ||
        _matchesAscii(bytes, offset, 'ANMF')) {
      return true;
    }
    if (_matchesAscii(bytes, offset, 'VP8X') &&
        chunkSize >= 1 &&
        (bytes[payloadOffset] & 0x02) != 0) {
      return true;
    }

    offset = payloadOffset + chunkSize + (chunkSize.isOdd ? 1 : 0);
  }

  return false;
}

bool _shouldSanitizePickedImage(String mimeType) {
  return _supportsNativeUploadImageProcessing() &&
      (mimeType == 'image/jpeg' || mimeType == 'image/png');
}

bool _supportsNativeUploadImageProcessing() {
  return switch (defaultTargetPlatform) {
    TargetPlatform.android || TargetPlatform.iOS => true,
    _ => false,
  };
}

bool _hasHeicFileExtension(XFile pickedImage) {
  for (final candidate in [pickedImage.name, pickedImage.path]) {
    final normalizedCandidate = candidate.toLowerCase();
    if (normalizedCandidate.endsWith('.heic') ||
        normalizedCandidate.endsWith('.heif')) {
      return true;
    }
  }
  return false;
}

bool _looksLikeHeicOrHeif(Uint8List bytes) {
  if (bytes.length < 12 || !_matchesAscii(bytes, 4, 'ftyp')) {
    return false;
  }

  final upperBound = bytes.length < 32 ? bytes.length : 32;
  for (var offset = 8; offset + 4 <= upperBound; offset += 4) {
    final brand = ascii.decode(
      bytes.sublist(offset, offset + 4),
      allowInvalid: true,
    );
    if (_heicBrands.contains(brand.toLowerCase())) {
      return true;
    }
  }

  return false;
}

bool _startsWith(Uint8List bytes, List<int> prefix) {
  if (bytes.length < prefix.length) return false;
  for (var i = 0; i < prefix.length; i++) {
    if (bytes[i] != prefix[i]) return false;
  }
  return true;
}

bool _matchesAscii(Uint8List bytes, int offset, String value) {
  final codeUnits = ascii.encode(value);
  if (bytes.length < offset + codeUnits.length) return false;
  for (var i = 0; i < codeUnits.length; i++) {
    if (bytes[offset + i] != codeUnits[i]) return false;
  }
  return true;
}

int _readUint32BigEndian(Uint8List bytes, int offset) {
  return (bytes[offset] << 24) |
      (bytes[offset + 1] << 16) |
      (bytes[offset + 2] << 8) |
      bytes[offset + 3];
}

int _readUint32LittleEndian(Uint8List bytes, int offset) {
  return bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24);
}

String? _extractServerAuthority(String baseUrl) {
  final uri = Uri.parse(baseUrl);
  if (uri.host.isEmpty) return null;
  final host = uri.host.contains(':') ? '[${uri.host}]' : uri.host;
  return uri.hasPort ? '$host:${uri.port}' : host;
}

Future<Uint8List> _transcodePickedImageToJpeg(Uint8List bytes) async {
  return _invokeRequiredPlatformBytesMethod(
    _transcodeImageToJpegMethod,
    arguments: bytes,
    errorMessage: 'failed to convert image for upload',
  );
}

Future<Uint8List> _sanitizePickedImageBytes(
  Uint8List bytes,
  String mimeType,
) async {
  return _invokeRequiredPlatformBytesMethod(
    _sanitizeImageForUploadMethod,
    arguments: {'bytes': bytes, 'mimeType': mimeType},
    errorMessage: 'failed to sanitize image for upload',
  );
}

Future<Uint8List> _invokeRequiredPlatformBytesMethod(
  String method, {
  Object? arguments,
  required String errorMessage,
}) async {
  final result = await _mediaUploadPlatformChannel.invokeMethod<Uint8List>(
    method,
    arguments,
  );
  if (result == null || result.isEmpty) {
    throw Exception(errorMessage);
  }
  return result;
}

final mediaUploadServiceProvider = Provider<MediaUploadService>((ref) {
  final config = ref.watch(relayConfigProvider);
  final picker = ImagePicker();
  final service = MediaUploadService(
    baseUrl: config.baseUrl,
    apiToken: config.apiToken,
    nsec: config.nsec,
    pickGalleryImage: () => picker.pickImage(
      source: ImageSource.gallery,
      requestFullMetadata: false,
    ),
  );
  ref.onDispose(service.dispose);
  return service;
});
