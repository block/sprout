import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'accent_colors.dart';

const _themeModeKey = 'sprout_theme_mode';
const _accentKey = 'sprout_accent_color';

/// Pre-loaded SharedPreferences instance, overridden in main().
final savedPrefsProvider = Provider<SharedPreferences>(
  (_) => throw UnimplementedError('Must be overridden'),
);

class ThemeNotifier extends Notifier<ThemeMode> {
  @override
  ThemeMode build() {
    final prefs = ref.read(savedPrefsProvider);
    final stored = prefs.getString(_themeModeKey);
    if (stored != null) {
      return ThemeMode.values.where((m) => m.name == stored).firstOrNull ??
          ThemeMode.system;
    }
    return ThemeMode.system;
  }

  void setThemeMode(ThemeMode themeMode) {
    state = themeMode;
    ref.read(savedPrefsProvider).setString(_themeModeKey, themeMode.name);
  }

  void toggleTheme() {
    switch (state) {
      case ThemeMode.light:
        setThemeMode(ThemeMode.dark);
        break;
      case ThemeMode.dark:
        setThemeMode(ThemeMode.system);
        break;
      case ThemeMode.system:
        setThemeMode(ThemeMode.light);
        break;
    }
  }
}

final themeProvider = NotifierProvider<ThemeNotifier, ThemeMode>(
  ThemeNotifier.new,
);

/// Tracks the selected accent color index.
/// -1 means "use theme default (Catppuccin Mauve)".
class AccentNotifier extends Notifier<int> {
  @override
  int build() {
    final prefs = ref.read(savedPrefsProvider);
    return prefs.getInt(_accentKey) ?? defaultAccentIndex;
  }

  void setAccent(int index) {
    state = index;
    ref.read(savedPrefsProvider).setInt(_accentKey, index);
  }
}

final accentProvider = NotifierProvider<AccentNotifier, int>(
  AccentNotifier.new,
);
