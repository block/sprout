import 'package:flutter/foundation.dart';
import 'package:intl/intl.dart';

// Re-export shortPubkey so existing callers continue to compile.
export '../../shared/utils/string_utils.dart' show shortPubkey;

final _fullDateFormat = DateFormat('EEEE, MMMM d, y');

/// Returns "Today", "Yesterday", or a full date like "Monday, March 31, 2026".
///
/// [now] is exposed for testing; production callers should omit it.
String formatDayHeading(int unixSeconds, {@visibleForTesting DateTime? now}) {
  final date = DateTime.fromMillisecondsSinceEpoch(
    unixSeconds * 1000,
    isUtc: true,
  ).toLocal();
  now ??= DateTime.now();
  final today = DateTime(now.year, now.month, now.day);
  final messageDay = DateTime(date.year, date.month, date.day);

  if (today.year == messageDay.year &&
      today.month == messageDay.month &&
      today.day == messageDay.day) {
    return 'Today';
  }
  final yesterday = DateTime(now.year, now.month, now.day - 1);
  if (yesterday.year == messageDay.year &&
      yesterday.month == messageDay.month &&
      yesterday.day == messageDay.day) {
    return 'Yesterday';
  }
  return _fullDateFormat.format(date);
}

/// Whether two unix-second timestamps fall on the same calendar day (local time).
bool isSameDay(int a, int b) {
  final dtA = DateTime.fromMillisecondsSinceEpoch(
    a * 1000,
    isUtc: true,
  ).toLocal();
  final dtB = DateTime.fromMillisecondsSinceEpoch(
    b * 1000,
    isUtc: true,
  ).toLocal();
  return dtA.year == dtB.year && dtA.month == dtB.month && dtA.day == dtB.day;
}

/// Returns a compact relative time string like "just now", "5m ago", "3h ago",
/// "2d ago", or a short date for older timestamps.
String relativeTime(int unixSeconds) {
  final now = DateTime.now();
  final time = DateTime.fromMillisecondsSinceEpoch(
    unixSeconds * 1000,
    isUtc: true,
  ).toLocal();
  final diff = now.difference(time);

  if (diff.inMinutes < 1) return 'just now';
  if (diff.inMinutes < 60) return '${diff.inMinutes}m ago';
  if (diff.inHours < 24) return '${diff.inHours}h ago';
  if (diff.inDays < 7) return '${diff.inDays}d ago';
  return '${time.month}/${time.day}/${time.year}';
}

/// Compact time label: "HH:MM" for today, "M/D HH:MM" for older messages.
String formatMessageTime(int unixSeconds) {
  final dt = DateTime.fromMillisecondsSinceEpoch(
    unixSeconds * 1000,
    isUtc: true,
  ).toLocal();
  final now = DateTime.now();
  final diff = now.difference(dt);

  final hh = dt.hour.toString().padLeft(2, '0');
  final mm = dt.minute.toString().padLeft(2, '0');

  if (diff.inDays > 0) {
    return '${dt.month}/${dt.day} $hh:$mm';
  }
  return '$hh:$mm';
}
