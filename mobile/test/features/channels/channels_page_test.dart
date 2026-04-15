import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:hooks_riverpod/misc.dart';
import 'package:sprout_mobile/features/channels/channel.dart';
import 'package:sprout_mobile/features/channels/channels_page.dart';
import 'package:sprout_mobile/features/channels/channels_provider.dart';
import 'package:sprout_mobile/features/profile/profile_provider.dart';
import 'package:sprout_mobile/features/profile/user_profile.dart';
import 'package:sprout_mobile/shared/theme/theme.dart';

void main() {
  Widget buildTestable({required List<Override> overrides}) {
    return ProviderScope(
      overrides: [
        // Provide a fake profile and presence so the avatar doesn't hit the network.
        profileProvider.overrideWith(() => _FakeProfileNotifier()),
        presenceProvider.overrideWith(() => _FakePresenceNotifier()),
        ...overrides,
      ],
      child: MaterialApp(
        theme: AppTheme.lightTheme,
        home: const ChannelsPage(),
      ),
    );
  }

  final testChannels = [
    Channel(
      id: '1',
      name: 'general',
      channelType: 'stream',
      visibility: 'open',
      description: 'General discussion',
      createdBy: 'abc',
      createdAt: DateTime(2025),
      memberCount: 10,
      isMember: true,
    ),
    Channel(
      id: '2',
      name: 'secret',
      channelType: 'stream',
      visibility: 'private',
      description: 'Private channel',
      createdBy: 'abc',
      createdAt: DateTime(2025),
      memberCount: 3,
      isMember: false,
    ),
  ];

  testWidgets('shows channel list when data loads', (tester) async {
    await tester.pumpWidget(
      buildTestable(
        overrides: [
          channelsProvider.overrideWith(() => _FakeNotifier(testChannels)),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('general'), findsOneWidget);
    expect(find.text('secret'), findsOneWidget);
    // Section header shows channel count
    expect(find.text('2'), findsOneWidget);
  });

  testWidgets('shows empty state when no channels', (tester) async {
    await tester.pumpWidget(
      buildTestable(
        overrides: [channelsProvider.overrideWith(() => _FakeNotifier([]))],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('No channels yet'), findsOneWidget);
  });

  testWidgets('shows error view with retry button', (tester) async {
    await tester.pumpWidget(
      buildTestable(
        overrides: [channelsProvider.overrideWith(() => _ErrorNotifier())],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Could not load channels'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);
  });
}

class _FakeNotifier extends ChannelsNotifier {
  final List<Channel> _channels;
  _FakeNotifier(this._channels);

  @override
  Future<List<Channel>> build() async => _channels;
}

class _ErrorNotifier extends ChannelsNotifier {
  @override
  Future<List<Channel>> build() => Future.error('Connection refused');
}

class _FakeProfileNotifier extends ProfileNotifier {
  @override
  Future<UserProfile?> build() async =>
      const UserProfile(pubkey: 'aabb', displayName: 'Test');
}

class _FakePresenceNotifier extends PresenceNotifier {
  @override
  Future<String> build() async => 'online';
}
