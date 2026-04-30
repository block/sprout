import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;

import '../../../shared/crypto/nip44.dart';
import '../../../shared/relay/relay.dart';
import 'observer_models.dart';
import 'transcript_builder.dart';

/// Maximum observer events to keep per agent (ring buffer cap).
const _maxObserverEvents = 800;

/// Key for the family provider.
typedef ObserverKey = ({String channelId, String agentPubkey});

/// State emitted by the observer subscription provider.
@immutable
class ObserverState {
  final ObserverConnectionState connection;
  final List<TranscriptItem> transcript;
  final String? errorMessage;

  const ObserverState({
    required this.connection,
    required this.transcript,
    this.errorMessage,
  });

  const ObserverState.initial()
    : connection = ObserverConnectionState.idle,
      transcript = const [],
      errorMessage = null;
}

class ObserverSubscriptionNotifier extends Notifier<ObserverState> {
  final ObserverKey _key;
  final List<ObserverFrame> _buffer = [];
  final Set<String> _dedupeKeys = {};
  void Function()? _unsubscribe;
  bool _disposed = false;

  ObserverSubscriptionNotifier(this._key);

  @override
  ObserverState build() {
    final sessionState = ref.watch(relaySessionProvider);

    _disposed = false;

    ref.onDispose(() {
      _disposed = true;
      _unsubscribe?.call();
      _unsubscribe = null;
      _buffer.clear();
      _dedupeKeys.clear();
    });

    if (sessionState.status == SessionStatus.connected) {
      _subscribeLive();
    }

    return const ObserverState.initial();
  }

  void _subscribeLive() async {
    final config = ref.read(relayConfigProvider);
    final nsec = config.nsec;
    if (nsec == null || nsec.isEmpty) return;

    String privHex;
    try {
      privHex = nostr.Nip19.decodePrivkey(nsec);
    } catch (_) {
      state = ObserverState(
        connection: ObserverConnectionState.error,
        transcript: state.transcript,
        errorMessage: 'Failed to decode private key',
      );
      return;
    }

    Uint8List conversationKey;
    try {
      conversationKey = getConversationKey(privHex, _key.agentPubkey);
    } catch (_) {
      state = ObserverState(
        connection: ObserverConnectionState.error,
        transcript: state.transcript,
        errorMessage: 'Failed to derive conversation key',
      );
      return;
    }

    // Derive our own pubkey from nsec via the nostr library.
    String myPubkey;
    try {
      myPubkey = nostr.Keychain(privHex).public;
    } catch (_) {
      state = ObserverState(
        connection: ObserverConnectionState.error,
        transcript: state.transcript,
        errorMessage: 'Failed to derive pubkey',
      );
      return;
    }

    state = ObserverState(
      connection: ObserverConnectionState.connecting,
      transcript: state.transcript,
    );

    final session = ref.read(relaySessionProvider.notifier);
    try {
      final unsub = await session.subscribe(
        NostrFilter(
          kinds: [24200],
          tags: {
            '#p': [myPubkey],
          },
          since: DateTime.now().millisecondsSinceEpoch ~/ 1000,
        ),
        (event) => _handleEvent(event, conversationKey, myPubkey),
      );

      // Guard against dispose during the async subscribe.
      if (_disposed) {
        unsub();
        return;
      }
      _unsubscribe = unsub;

      state = ObserverState(
        connection: ObserverConnectionState.open,
        transcript: state.transcript,
      );
    } catch (e) {
      state = ObserverState(
        connection: ObserverConnectionState.error,
        transcript: state.transcript,
        errorMessage: 'Subscription failed: $e',
      );
    }
  }

  void _handleEvent(
    NostrEvent event,
    Uint8List conversationKey,
    String myPubkey,
  ) {
    // Filter to only this agent's frames.
    if (event.pubkey.toLowerCase() != _key.agentPubkey.toLowerCase()) return;

    // Defense-in-depth: verify event.pubkey matches claimed agent tag.
    final agentTag = event.getTagValue('agent');
    if (agentTag == null ||
        event.pubkey.toLowerCase() != agentTag.toLowerCase()) {
      return;
    }

    // Must be a telemetry frame.
    if (event.getTagValue('frame') != 'telemetry') return;

    ObserverFrame frame;
    try {
      final plaintext = nip44Decrypt(conversationKey, event.content);
      final json = jsonDecode(plaintext) as Map<String, dynamic>;
      frame = ObserverFrame.fromJson(json);
    } catch (e) {
      state = ObserverState(
        connection: ObserverConnectionState.error,
        transcript: state.transcript,
        errorMessage: 'Decrypt failed: $e',
      );
      return;
    }

    // Deduplicate by (seq, timestamp).
    final dedupeKey = '${frame.seq}:${frame.timestamp}';
    if (_dedupeKeys.contains(dedupeKey)) return;
    _dedupeKeys.add(dedupeKey);

    // Scope to this channel (null channelId = not channel-scoped, let through).
    if (frame.channelId != null && frame.channelId != _key.channelId) return;

    // Add to buffer and sort.
    _buffer.add(frame);
    _buffer.sort(_compareObserverEvents);

    // Cap buffer size.
    if (_buffer.length > _maxObserverEvents) {
      final removed = _buffer.sublist(0, _buffer.length - _maxObserverEvents);
      for (final r in removed) {
        _dedupeKeys.remove('${r.seq}:${r.timestamp}');
      }
      _buffer.removeRange(0, _buffer.length - _maxObserverEvents);
    }

    // Rebuild transcript.
    state = ObserverState(
      connection: ObserverConnectionState.open,
      transcript: buildTranscript(_buffer),
    );
  }

  /// Compare observer events: primary by timestamp, secondary by seq.
  static int _compareObserverEvents(ObserverFrame a, ObserverFrame b) {
    final tsA = DateTime.tryParse(a.timestamp)?.millisecondsSinceEpoch ?? 0;
    final tsB = DateTime.tryParse(b.timestamp)?.millisecondsSinceEpoch ?? 0;
    if (tsA != tsB) return tsA.compareTo(tsB);
    return a.seq.compareTo(b.seq);
  }
}

final observerSubscriptionProvider =
    NotifierProvider.family<
      ObserverSubscriptionNotifier,
      ObserverState,
      ObserverKey
    >(ObserverSubscriptionNotifier.new);
