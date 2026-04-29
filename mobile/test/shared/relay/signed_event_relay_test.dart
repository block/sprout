import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:nostr/nostr.dart' as nostr;
import 'package:sprout_mobile/shared/relay/relay_client.dart';
import 'package:sprout_mobile/shared/relay/signed_event_relay.dart';

void main() {
  group('SignedEventRelay', () {
    test('throws when nsec is null', () {
      final relay = SignedEventRelay(
        client: RelayClient(baseUrl: 'http://localhost'),
        nsec: null,
      );

      expect(
        () => relay.submit(kind: 1, content: 'hi', tags: []),
        throwsA(
          isA<Exception>().having(
            (e) => e.toString(),
            'message',
            contains('no signing key'),
          ),
        ),
      );
    });

    test('throws when nsec is empty', () {
      final relay = SignedEventRelay(
        client: RelayClient(baseUrl: 'http://localhost'),
        nsec: '',
      );

      expect(
        () => relay.submit(kind: 1, content: 'hi', tags: []),
        throwsA(
          isA<Exception>().having(
            (e) => e.toString(),
            'message',
            contains('no signing key'),
          ),
        ),
      );
    });

    test('posts signed event and succeeds when accepted', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      Map<String, dynamic>? postedBody;
      final mockHttp = http_testing.MockClient((request) async {
        expect(request.url.path, '/api/events');
        postedBody = jsonDecode(request.body) as Map<String, dynamic>;
        return http.Response(jsonEncode({'accepted': true}), 200);
      });

      final client = RelayClient(
        baseUrl: 'http://localhost',
        httpClient: mockHttp,
      );
      final relay = SignedEventRelay(client: client, nsec: nsec);

      await relay.submit(
        kind: 9007,
        content: 'test message',
        createdAt: 1234567890,
        tags: [
          ['h', 'channel-1'],
        ],
      );

      expect(postedBody, isNotNull);
      expect(postedBody!['kind'], 9007);
      expect(postedBody!['content'], 'test message');
      expect(postedBody!['created_at'], 1234567890);
      expect(postedBody!['sig'], isNotEmpty);
      expect(postedBody!['pubkey'], keychain.public);
    });

    test('throws when relay rejects event', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final mockHttp = http_testing.MockClient((request) async {
        return http.Response(
          jsonEncode({'accepted': false, 'message': 'invalid event'}),
          200,
        );
      });

      final client = RelayClient(
        baseUrl: 'http://localhost',
        httpClient: mockHttp,
      );
      final relay = SignedEventRelay(client: client, nsec: nsec);

      expect(
        () => relay.submit(kind: 1, content: '', tags: []),
        throwsA(
          isA<Exception>().having(
            (e) => e.toString(),
            'message',
            contains('invalid event'),
          ),
        ),
      );
    });

    test('throws generic message when relay rejects without message', () async {
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);

      final mockHttp = http_testing.MockClient((request) async {
        return http.Response(jsonEncode({'accepted': false}), 200);
      });

      final client = RelayClient(
        baseUrl: 'http://localhost',
        httpClient: mockHttp,
      );
      final relay = SignedEventRelay(client: client, nsec: nsec);

      expect(
        () => relay.submit(kind: 1, content: '', tags: []),
        throwsA(
          isA<Exception>().having(
            (e) => e.toString(),
            'message',
            contains('Event rejected by relay'),
          ),
        ),
      );
    });
  });
}
