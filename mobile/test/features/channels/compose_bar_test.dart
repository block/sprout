import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:image_picker/image_picker.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:sprout_mobile/features/channels/channel_management_provider.dart';
import 'package:sprout_mobile/features/channels/compose_bar.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';
import 'package:sprout_mobile/shared/theme/theme.dart';

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

const _mediaUploadPlatformChannel = MethodChannel('sprout/media_upload');

void _setMockMediaUploadPlatformHandler(
  Future<Object?> Function(MethodCall call)? handler,
) {
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(_mediaUploadPlatformChannel, handler);
}

Widget _buildComposeBar({
  required MediaUploadService uploadService,
  required ComposeBarOnSend onSend,
}) {
  return ProviderScope(
    overrides: [
      mediaUploadServiceProvider.overrideWithValue(uploadService),
      currentPubkeyProvider.overrideWith((ref) => null),
      channelMembersProvider(
        'channel-1',
      ).overrideWith((ref) async => const <ChannelMember>[]),
      relayClientProvider.overrideWithValue(
        RelayClient(baseUrl: 'http://localhost:3000'),
      ),
    ],
    child: MaterialApp(
      theme: AppTheme.lightTheme,
      home: Scaffold(
        body: SafeArea(
          child: ComposeBar(channelId: 'channel-1', onSend: onSend),
        ),
      ),
    ),
  );
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    _setMockMediaUploadPlatformHandler((call) async {
      switch (call.method) {
        case 'sanitizeImageForUpload':
          final arguments = call.arguments as Map<Object?, Object?>;
          return arguments['bytes'] as Uint8List;
        case 'transcodeImageToJpeg':
          return _pngBytes;
        default:
          return null;
      }
    });
  });

  tearDownAll(() {
    _setMockMediaUploadPlatformHandler(null);
  });

  group('ComposeBar', () {
    testWidgets('uploads an image and sends markdown plus imeta tags', (
      tester,
    ) async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final uploadService = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        httpClient: http_testing.MockClient((request) async {
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
        }),
        pickGalleryImage: () async =>
            XFile.fromData(_pngBytes, name: 'tiny.png'),
      );

      String? sentContent;
      List<List<String>> sentMediaTags = const [];
      await tester.pumpWidget(
        _buildComposeBar(
          uploadService: uploadService,
          onSend:
              (
                content,
                mentionPubkeys, {
                mediaTags = const <List<String>>[],
              }) async {
                sentContent = content;
                sentMediaTags = mediaTags;
              },
        ),
      );

      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(find.byTooltip('Remove attachment'), findsOneWidget);

      await tester.tap(find.byIcon(LucideIcons.sendHorizontal));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(sentContent, '\n![image](https://relay.example/media/test.png)');
      expect(sentMediaTags, hasLength(1));
      expect(sentMediaTags.first.first, 'imeta');
      expect(
        sentMediaTags.first,
        contains('url https://relay.example/media/test.png'),
      );
      expect(find.byTooltip('Remove attachment'), findsNothing);
    });

    testWidgets('keeps the remove button pinned to the attachment corner', (
      tester,
    ) async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final uploadService = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        httpClient: http_testing.MockClient((request) async {
          return http.Response(
            jsonEncode({
              'url': 'https://relay.example/media/test.png',
              'sha256':
                  '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
              'size': 16,
              'type': 'image/png',
              'uploaded': 1,
            }),
            200,
          );
        }),
        pickGalleryImage: () async =>
            XFile.fromData(_pngBytes, name: 'tiny.png'),
      );

      await tester.pumpWidget(
        _buildComposeBar(
          uploadService: uploadService,
          onSend:
              (
                content,
                mentionPubkeys, {
                mediaTags = const <List<String>>[],
              }) async {},
        ),
      );

      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await tester.pump();
      await tester.pumpAndSettle();

      final attachmentFinder = find.byKey(
        const ValueKey(
          'compose-attachment:https://relay.example/media/test.png',
        ),
      );
      final removeButtonFinder = find.byTooltip('Remove attachment');

      expect(attachmentFinder, findsOneWidget);
      expect(removeButtonFinder, findsOneWidget);

      final attachmentTopRight = tester.getTopRight(attachmentFinder);
      final attachmentTopLeft = tester.getTopLeft(attachmentFinder);
      final removeButtonCenter = tester.getCenter(removeButtonFinder);

      expect(
        attachmentTopRight.dx - removeButtonCenter.dx,
        lessThanOrEqualTo(16),
      );
      expect(
        removeButtonCenter.dy - attachmentTopLeft.dy,
        lessThanOrEqualTo(16),
      );
    });

    testWidgets('shows an upload error when gallery upload fails', (
      tester,
    ) async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final uploadService = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        httpClient: http_testing.MockClient((request) async {
          return http.Response('bad upload', 401);
        }),
        pickGalleryImage: () async =>
            XFile.fromData(_pngBytes, name: 'tiny.png'),
      );

      await tester.pumpWidget(
        _buildComposeBar(
          uploadService: uploadService,
          onSend:
              (
                content,
                mentionPubkeys, {
                mediaTags = const <List<String>>[],
              }) async {},
        ),
      );

      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(find.textContaining('upload failed'), findsOneWidget);
    });

    testWidgets('shows a clean error when a GIF is picked', (tester) async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final uploadService = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        pickGalleryImage: () async =>
            XFile.fromData(_gifBytes, name: 'animated.gif'),
      );

      await tester.pumpWidget(
        _buildComposeBar(
          uploadService: uploadService,
          onSend:
              (
                content,
                mentionPubkeys, {
                mediaTags = const <List<String>>[],
              }) async {},
        ),
      );

      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(
        find.textContaining('GIF uploads are not supported on mobile yet'),
        findsOneWidget,
      );
    });

    testWidgets('shows a clean error when an animated PNG is picked', (
      tester,
    ) async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final uploadService = MediaUploadService(
        baseUrl: 'https://relay.example',
        apiToken: 'sprout_test_token',
        nsec: nsec,
        pickGalleryImage: () async =>
            XFile.fromData(_apngBytes, name: 'animated.png'),
      );

      await tester.pumpWidget(
        _buildComposeBar(
          uploadService: uploadService,
          onSend:
              (
                content,
                mentionPubkeys, {
                mediaTags = const <List<String>>[],
              }) async {},
        ),
      );

      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(
        find.textContaining('Animated PNG uploads are not supported on mobile'),
        findsOneWidget,
      );
    });
  });
}
