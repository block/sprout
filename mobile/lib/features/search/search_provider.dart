import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../channels/channel.dart';
import '../channels/channel_management_provider.dart';
import '../channels/channels_provider.dart';

@immutable
class SearchHit {
  final String eventId;
  final String content;
  final int kind;
  final String pubkey;
  final String? channelId;
  final String? channelName;
  final int createdAt;
  final double score;

  const SearchHit({
    required this.eventId,
    required this.content,
    required this.kind,
    required this.pubkey,
    this.channelId,
    this.channelName,
    required this.createdAt,
    required this.score,
  });

  factory SearchHit.fromJson(Map<String, dynamic> json) => SearchHit(
    eventId: json['event_id'] as String,
    content: json['content'] as String? ?? '',
    kind: json['kind'] as int? ?? 9,
    pubkey: json['pubkey'] as String,
    channelId: json['channel_id'] as String?,
    channelName: json['channel_name'] as String?,
    createdAt: json['created_at'] as int? ?? 0,
    score: (json['score'] as num?)?.toDouble() ?? 0.0,
  );
}

@immutable
class SearchState {
  final String query;
  final List<SearchHit> messageResults;
  final List<DirectoryUser> userResults;
  final List<Channel> channelResults;
  final bool isLoading;
  final String? error;

  const SearchState({
    this.query = '',
    this.messageResults = const [],
    this.userResults = const [],
    this.channelResults = const [],
    this.isLoading = false,
    this.error,
  });

  const SearchState.initial()
    : query = '',
      messageResults = const [],
      userResults = const [],
      channelResults = const [],
      isLoading = false,
      error = null;

  SearchState copyWith({
    String? query,
    List<SearchHit>? messageResults,
    List<DirectoryUser>? userResults,
    List<Channel>? channelResults,
    bool? isLoading,
    String? error,
  }) => SearchState(
    query: query ?? this.query,
    messageResults: messageResults ?? this.messageResults,
    userResults: userResults ?? this.userResults,
    channelResults: channelResults ?? this.channelResults,
    isLoading: isLoading ?? this.isLoading,
    error: error ?? this.error,
  );
}

class SearchNotifier extends Notifier<SearchState> {
  Timer? _debounce;

  @override
  SearchState build() {
    // Watch relayClientProvider so search state resets on workspace switch.
    ref.watch(relayClientProvider);
    ref.onDispose(() {
      _debounce?.cancel();
    });
    return const SearchState.initial();
  }

  void search(String query) {
    _debounce?.cancel();

    final trimmed = query.trim();
    if (trimmed.isEmpty) {
      state = const SearchState.initial();
      return;
    }

    state = SearchState(query: trimmed, isLoading: true);

    _debounce = Timer(const Duration(milliseconds: 300), () {
      _executeSearch(trimmed);
    });
  }

  Future<void> _executeSearch(String query) async {
    // If the query changed while debouncing, bail out.
    if (state.query != query) return;

    // Fire all three lookups in parallel.
    _searchMessages(query);
    _searchUsers(query);
    _searchChannels(query);
  }

  Future<void> _searchMessages(String query) async {
    try {
      final client = ref.read(relayClientProvider);
      final json =
          await client.get(
                '/api/search',
                queryParams: {'q': query, 'limit': '20'},
              )
              as Map<String, dynamic>;

      final hits = (json['hits'] as List<dynamic>? ?? [])
          .cast<Map<String, dynamic>>()
          .map(SearchHit.fromJson)
          .toList();

      if (state.query != query) return;
      state = state.copyWith(messageResults: hits, isLoading: false);
    } catch (e) {
      if (state.query != query) return;
      state = state.copyWith(isLoading: false, error: e.toString());
    }
  }

  Future<void> _searchUsers(String query) async {
    try {
      final users = await ref
          .read(channelActionsProvider)
          .searchUsers(query, limit: 8);

      if (state.query != query) return;
      state = state.copyWith(
        userResults: users,
        isLoading: state.messageResults.isEmpty && state.channelResults.isEmpty,
      );
    } catch (_) {
      // User search failure is non-critical — keep existing results.
    }
  }

  void _searchChannels(String query) {
    final channels = ref.read(channelsProvider).value ?? [];
    final lowerQuery = query.toLowerCase();
    final matches = channels.where((c) {
      if (c.isDm) return false;
      return c.name.toLowerCase().contains(lowerQuery) ||
          c.description.toLowerCase().contains(lowerQuery);
    }).toList();

    if (state.query != query) return;
    state = state.copyWith(
      channelResults: matches,
      isLoading: state.messageResults.isEmpty && state.userResults.isEmpty,
    );
  }

  void clear() {
    _debounce?.cancel();
    state = const SearchState.initial();
  }
}

final searchProvider = NotifierProvider<SearchNotifier, SearchState>(
  SearchNotifier.new,
);
