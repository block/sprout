import 'dart:convert';

import 'package:http/http.dart' as http;

/// Lightweight HTTP client for talking to the Sprout relay REST API.
class RelayClient {
  final String baseUrl;
  final String? apiToken;
  final String? devPubkey;
  final http.Client _http;

  RelayClient({
    required this.baseUrl,
    this.apiToken,
    this.devPubkey,
    http.Client? httpClient,
  }) : _http = httpClient ?? http.Client();

  Map<String, String> get _headers {
    final h = {'Content-Type': 'application/json'};
    if (apiToken case final token?) {
      h['Authorization'] = 'Bearer $token';
    } else if (devPubkey case final pk?) {
      h['X-Pubkey'] = pk;
    }
    return h;
  }

  Uri _uri(String path, {Map<String, String>? queryParams}) {
    final base = Uri.parse(baseUrl);
    // Resolve path against base to avoid double-slash issues.
    final resolved = base.resolve(path);
    if (queryParams?.isNotEmpty == true) {
      return resolved.replace(queryParameters: queryParams);
    }
    return resolved;
  }

  /// GET [path] and return decoded JSON.
  Future<dynamic> get(String path, {Map<String, String>? queryParams}) async {
    final response = await _http.get(
      _uri(path, queryParams: queryParams),
      headers: _headers,
    );
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw RelayException(response.statusCode, response.body);
    }
    return jsonDecode(response.body);
  }

  /// POST [path] with a JSON [body] and return decoded JSON, or null for
  /// empty responses (e.g. 204).
  Future<dynamic> post(String path, {Object? body}) async {
    final response = await _http.post(
      _uri(path),
      headers: _headers,
      body: body != null ? jsonEncode(body) : null,
    );
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw RelayException(response.statusCode, response.body);
    }
    if (response.body.isEmpty) return null;
    return jsonDecode(response.body);
  }

  void dispose() => _http.close();
}

class RelayException implements Exception {
  final int statusCode;
  final String body;

  RelayException(this.statusCode, this.body);

  @override
  String toString() => 'RelayException($statusCode): $body';
}
