import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:image_picker/image_picker.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:sprout_mobile/shared/relay/media_upload.dart';

final _pngBytes = Uint8List.fromList([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x0d,
  0x49,
  0x48,
  0x44,
  0x52,
]);

final _jpegBytes = Uint8List.fromList([
  0xff,
  0xd8,
  0xff,
  0xdb,
  0x00,
  0x43,
  0x00,
  0x01,
]);

final _heicBytes = Uint8List.fromList([
  0x00,
  0x00,
  0x00,
  0x18,
  0x66,
  0x74,
  0x79,
  0x70,
  0x68,
  0x65,
  0x69,
  0x63,
  0x00,
  0x00,
  0x00,
  0x00,
  0x6d,
  0x69,
  0x66,
  0x31,
  0x68,
  0x65,
  0x69,
  0x63,
]);

final _gifBytes = Uint8List.fromList([
  0x47,
  0x49,
  0x46,
  0x38,
  0x39,
  0x61,
  0x01,
  0x00,
  0x01,
  0x00,
]);

final _apngBytes = Uint8List.fromList([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x08,
  0x61,
  0x63,
  0x54,
  0x4c,
  0x00,
  0x00,
  0x00,
  0x02,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x49,
  0x45,
  0x4e,
  0x44,
  0x00,
  0x00,
  0x00,
  0x00,
]);

final _staticPngWithActlPayloadBytes = Uint8List.fromList([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x04,
  0x49,
  0x44,
  0x41,
  0x54,
  0x61,
  0x63,
  0x54,
  0x4c,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x49,
  0x45,
  0x4e,
  0x44,
  0x00,
  0x00,
  0x00,
  0x00,
]);

final _animatedWebpBytes = Uint8List.fromList([
  0x52,
  0x49,
  0x46,
  0x46,
  0x16,
  0x00,
  0x00,
  0x00,
  0x57,
  0x45,
  0x42,
  0x50,
  0x56,
  0x50,
  0x38,
  0x58,
  0x0a,
  0x00,
  0x00,
  0x00,
  0x02,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
]);

void main() {
  group('MediaUploadService', () {
    test('signs Blossom auth and uploads gallery image bytes', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      http.Request? capturedRequest;
      final client = http_testing.MockClient((request) async {
        capturedRequest = request;
        return http.Response(
          jsonEncode({
            'url': 'https://relay.example/media/test.png',
            'sha256':
                '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            'size': 16,
            'type': 'image/png',
            'uploaded': 1,
            'thumb': 'https://relay.example/media/test.thumb.jpg',
          }),
          200,
        );
      });

      final service = MediaUploadService(
        baseUrl: 'https://relay.example:8443',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        httpClient: client,
        pickGalleryImage: () async =>
            XFile.fromData(_pngBytes, name: 'tiny.png'),
        now: () => DateTime.fromMillisecondsSinceEpoch(1_700_000_000_000),
      );

      final descriptor = await service.pickAndUploadImage();

      expect(descriptor, isNotNull);
      expect(descriptor!.type, 'image/png');
      expect(capturedRequest, isNotNull);
      expect(
        capturedRequest!.url.toString(),
        'https://relay.example:8443/media/upload',
      );
      expect(capturedRequest!.headers['Content-Type'], 'image/png');
      expect(capturedRequest!.headers['X-Auth-Token'], 'sprout_test_token');
      expect(capturedRequest!.headers['X-SHA-256'], isNotEmpty);
      expect(capturedRequest!.bodyBytes, _pngBytes);

      final authHeader = capturedRequest!.headers['Authorization'];
      expect(authHeader, isNotNull);
      expect(authHeader, startsWith('Nostr '));
      final encoded = authHeader!.substring('Nostr '.length);
      final decoded = utf8.decode(
        base64Url.decode(base64Url.normalize(encoded)),
      );
      final authEvent = jsonDecode(decoded) as Map<String, dynamic>;
      final tags = (authEvent['tags'] as List<dynamic>)
          .map((tag) => (tag as List<dynamic>).cast<String>())
          .toList();

      expect(authEvent['kind'], 24242);
      expect(authEvent['pubkey'], keychain.public);
      expect(tags, anyElement(equals(<String>['t', 'upload'])));
      expect(
        tags,
        anyElement(
          equals(<String>['x', capturedRequest!.headers['X-SHA-256']!]),
        ),
      );
      expect(tags, anyElement(equals(<String>['expiration', '1700000300'])));
      expect(
        tags,
        anyElement(equals(<String>['server', 'relay.example:8443'])),
      );
    });

    test('returns null when the gallery picker is cancelled', () async {
      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: null,
        pickGalleryImage: () async => null,
      );

      final result = await service.pickAndUploadImage();
      expect(result, isNull);
    });

    test('uses a bracketed IPv6 server tag in Blossom auth', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      http.Request? capturedRequest;
      final client = http_testing.MockClient((request) async {
        capturedRequest = request;
        return http.Response(
          jsonEncode({
            'url': 'http://[::1]:3000/media/test.png',
            'sha256':
                '2222222222222222222222222222222222222222222222222222222222222222',
            'size': 16,
            'type': 'image/png',
            'uploaded': 1,
          }),
          200,
        );
      });

      final service = MediaUploadService(
        baseUrl: 'http://[::1]:3000',
        apiToken: null,
        nsec: nsec,
        httpClient: client,
        pickGalleryImage: () async =>
            XFile.fromData(_pngBytes, name: 'tiny.png'),
      );

      await service.pickAndUploadImage();

      expect(capturedRequest, isNotNull);
      final authHeader = capturedRequest!.headers['Authorization'];
      expect(authHeader, isNotNull);
      final encoded = authHeader!.substring('Nostr '.length);
      final decoded = utf8.decode(
        base64Url.decode(base64Url.normalize(encoded)),
      );
      final authEvent = jsonDecode(decoded) as Map<String, dynamic>;
      final tags = (authEvent['tags'] as List<dynamic>)
          .map((tag) => (tag as List<dynamic>).cast<String>())
          .toList();

      expect(tags, anyElement(equals(<String>['server', '[::1]:3000'])));
    });

    test('transcodes HEIC gallery files on iOS before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final previousPlatform = debugDefaultTargetPlatformOverride;
      debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
      addTearDown(() {
        debugDefaultTargetPlatformOverride = previousPlatform;
      });

      Uint8List? transcodedInput;
      http.Request? capturedRequest;
      final client = http_testing.MockClient((request) async {
        capturedRequest = request;
        return http.Response(
          jsonEncode({
            'url': 'https://relay.example/media/test.jpg',
            'sha256':
                'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210',
            'size': _jpegBytes.length,
            'type': 'image/jpeg',
            'uploaded': 1,
          }),
          200,
        );
      });

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        httpClient: client,
        pickGalleryImage: () async =>
            XFile.fromData(_heicBytes, name: 'photo.heic'),
        transcodeImageToJpeg: (bytes) async {
          transcodedInput = bytes;
          return _jpegBytes;
        },
      );

      final descriptor = await service.pickAndUploadImage();

      expect(descriptor, isNotNull);
      expect(descriptor!.type, 'image/jpeg');
      expect(transcodedInput, _heicBytes);
      expect(capturedRequest, isNotNull);
      expect(capturedRequest!.headers['Content-Type'], 'image/jpeg');
      expect(capturedRequest!.bodyBytes, _jpegBytes);
    });

    test('sanitizes iOS JPEG gallery files before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final previousPlatform = debugDefaultTargetPlatformOverride;
      debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
      addTearDown(() {
        debugDefaultTargetPlatformOverride = previousPlatform;
      });

      Uint8List? sanitizedInput;
      String? sanitizedMimeType;
      http.Request? capturedRequest;
      final client = http_testing.MockClient((request) async {
        capturedRequest = request;
        return http.Response(
          jsonEncode({
            'url': 'https://relay.example/media/test.jpg',
            'sha256':
                'abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd',
            'size': _jpegBytes.length,
            'type': 'image/jpeg',
            'uploaded': 1,
          }),
          200,
        );
      });

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        httpClient: client,
        pickGalleryImage: () async =>
            XFile.fromData(_jpegBytes, name: 'photo.jpg'),
        sanitizeImageBytes: (bytes, mimeType) async {
          sanitizedInput = bytes;
          sanitizedMimeType = mimeType;
          return _jpegBytes;
        },
      );

      final descriptor = await service.pickAndUploadImage();

      expect(descriptor, isNotNull);
      expect(descriptor!.type, 'image/jpeg');
      expect(sanitizedInput, _jpegBytes);
      expect(sanitizedMimeType, 'image/jpeg');
      expect(capturedRequest, isNotNull);
      expect(capturedRequest!.headers['Content-Type'], 'image/jpeg');
      expect(capturedRequest!.bodyBytes, _jpegBytes);
    });

    test('rejects GIF gallery files before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        pickGalleryImage: () async =>
            XFile.fromData(_gifBytes, name: 'animated.gif'),
      );

      expect(
        service.pickAndUploadImage(),
        throwsA(
          isA<Exception>().having(
            (error) => error.toString(),
            'message',
            contains('GIF uploads are not supported on mobile yet'),
          ),
        ),
      );
    });

    test('rejects animated PNG gallery files before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        httpClient: http_testing.MockClient(
          (request) async => http.Response('{}', 200),
        ),
        pickGalleryImage: () async =>
            XFile.fromData(_apngBytes, name: 'animated.png'),
      );

      expect(
        service.pickAndUploadImage(),
        throwsA(
          isA<Exception>().having(
            (error) => error.toString(),
            'message',
            contains('Animated PNG uploads are not supported on mobile yet'),
          ),
        ),
      );
    });

    test('uploads static PNG when acTL appears only in chunk payload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      http.Request? capturedRequest;
      final client = http_testing.MockClient((request) async {
        capturedRequest = request;
        return http.Response(
          jsonEncode({
            'url': 'https://relay.example/media/static.png',
            'sha256':
                '1111111111111111111111111111111111111111111111111111111111111111',
            'size': _staticPngWithActlPayloadBytes.length,
            'type': 'image/png',
            'uploaded': 1,
          }),
          200,
        );
      });

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        httpClient: client,
        pickGalleryImage: () async =>
            XFile.fromData(_staticPngWithActlPayloadBytes, name: 'static.png'),
      );

      final descriptor = await service.pickAndUploadImage();

      expect(descriptor, isNotNull);
      expect(descriptor!.type, 'image/png');
      expect(capturedRequest, isNotNull);
      expect(capturedRequest!.headers['Content-Type'], 'image/png');
      expect(capturedRequest!.bodyBytes, _staticPngWithActlPayloadBytes);
    });

    test('rejects animated WebP gallery files before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        httpClient: http_testing.MockClient(
          (request) async => http.Response('{}', 200),
        ),
        pickGalleryImage: () async =>
            XFile.fromData(_animatedWebpBytes, name: 'animated.webp'),
      );

      expect(
        service.pickAndUploadImage(),
        throwsA(
          isA<Exception>().having(
            (error) => error.toString(),
            'message',
            contains('Animated WebP uploads are not supported on mobile yet'),
          ),
        ),
      );
    });

    test('rejects unsupported gallery files before upload', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final service = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: null,
        nsec: nsec,
        pickGalleryImage: () async => XFile.fromData(
          Uint8List.fromList(utf8.encode('not an image')),
          name: 'note.txt',
        ),
      );

      expect(
        service.pickAndUploadImage(),
        throwsA(
          isA<Exception>().having(
            (error) => error.toString(),
            'message',
            contains('unsupported file type'),
          ),
        ),
      );
    });
  });
}
