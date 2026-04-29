import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:sprout_mobile/shared/relay/relay_client.dart';

void main() {
  group('RelayClient', () {
    test('GET sends auth header and parses JSON', () async {
      final mockClient = http_testing.MockClient((request) async {
        expect(request.url.toString(), 'http://test:3000/api/channels');
        expect(request.headers['Authorization'], 'Bearer sprout_abc');
        expect(request.headers['Content-Type'], 'application/json');
        return http.Response(
          jsonEncode([
            {'id': '1', 'name': 'general'},
          ]),
          200,
        );
      });

      final client = RelayClient(
        baseUrl: 'http://test:3000',
        apiToken: 'sprout_abc',
        httpClient: mockClient,
      );

      final result = await client.get('/api/channels');
      expect(result, isList);
      expect((result as List).first['name'], 'general');
    });

    test('GET with query parameters', () async {
      final mockClient = http_testing.MockClient((request) async {
        expect(request.url.queryParameters['visibility'], 'open');
        return http.Response(jsonEncode([]), 200);
      });

      final client = RelayClient(
        baseUrl: 'http://test:3000',
        httpClient: mockClient,
      );

      await client.get('/api/channels', queryParams: {'visibility': 'open'});
    });

    test('throws RelayException on non-200', () async {
      final mockClient = http_testing.MockClient((request) async {
        return http.Response('{"error": "unauthorized"}', 401);
      });

      final client = RelayClient(
        baseUrl: 'http://test:3000',
        httpClient: mockClient,
      );

      expect(
        () => client.get('/api/channels'),
        throwsA(
          isA<RelayException>().having((e) => e.statusCode, 'statusCode', 401),
        ),
      );
    });

    test('RelayException string includes non-empty response body', () {
      final exception = RelayException(
        403,
        '{"message":"missing users:write"}',
      );

      expect(
        exception.toString(),
        'RelayException(403): {"message":"missing users:write"}',
      );
    });

    test('RelayException string omits empty response body', () {
      final exception = RelayException(403, '   ');

      expect(exception.toString(), 'RelayException(403)');
    });

    test('omits Authorization header when no token', () async {
      final mockClient = http_testing.MockClient((request) async {
        expect(request.headers.containsKey('Authorization'), isFalse);
        return http.Response(jsonEncode({}), 200);
      });

      final client = RelayClient(
        baseUrl: 'http://test:3000',
        httpClient: mockClient,
      );

      await client.get('/api/test');
    });
  });
}
