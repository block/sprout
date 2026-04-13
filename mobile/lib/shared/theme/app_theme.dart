import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'app_colors.dart';
import 'color_scheme.dart';
import 'grid.dart';
import 'text_theme.dart';

class AppTheme {
  static ThemeData get lightTheme {
    const appColors = AppColors(
      success: Color(0xFF40A02B), // Latte Green
      warning: Color(0xFFDF8E1D), // Latte Yellow
      accent: Color(0xFF1E66F5), // Latte Blue
    );

    return ThemeData(
      useMaterial3: true,
      colorScheme: lightColorScheme,
      extensions: [appColors],
      textTheme: textTheme,
      appBarTheme: AppBarTheme(
        backgroundColor: lightColorScheme.surface,
        foregroundColor: lightColorScheme.onSurface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        titleTextStyle: textTheme.titleMedium?.copyWith(
          color: lightColorScheme.onSurface,
        ),
        systemOverlayStyle: const SystemUiOverlayStyle(
          statusBarColor: Colors.transparent,
          statusBarIconBrightness: Brightness.dark,
          statusBarBrightness: Brightness.light,
        ),
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          backgroundColor: lightColorScheme.primary,
          foregroundColor: lightColorScheme.onPrimary,
          elevation: 0,
          padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(24),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          backgroundColor: lightColorScheme.surface,
          foregroundColor: lightColorScheme.onSurface,
          side: BorderSide(color: lightColorScheme.outline, width: 1),
          padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(24),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: lightColorScheme.onSurface,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        ),
      ),
      cardTheme: CardThemeData(
        color: lightColorScheme.surfaceContainerHighest,
        margin: EdgeInsets.zero,
        elevation: 0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: lightColorScheme.surfaceContainerHighest,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: lightColorScheme.outline),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: lightColorScheme.outline),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: lightColorScheme.primary),
        ),
        errorBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: lightColorScheme.error),
        ),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 16,
          vertical: 12,
        ),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        strokeWidth: 2,
        color: appColors.accent,
        circularTrackColor: lightColorScheme.onSurfaceVariant.withValues(
          alpha: 0.2,
        ),
      ),
      listTileTheme: ListTileThemeData(
        titleTextStyle: textTheme.titleSmall?.copyWith(
          color: lightColorScheme.onSurface,
        ),
        subtitleTextStyle: textTheme.bodyMedium?.copyWith(
          color: lightColorScheme.secondary,
        ),
        iconColor: lightColorScheme.secondary,
        contentPadding: const EdgeInsets.symmetric(horizontal: Grid.twelve),
        minVerticalPadding: Grid.twelve,
        horizontalTitleGap: Grid.twelve,
      ),
      chipTheme: ChipThemeData(
        labelStyle: textTheme.bodySmall?.copyWith(
          color: lightColorScheme.secondary,
        ),
        shape: RoundedRectangleBorder(
          side: BorderSide.none,
          borderRadius: BorderRadius.circular(8),
        ),
        side: BorderSide.none,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        labelPadding: EdgeInsets.zero,
      ),
    );
  }

  static ThemeData get darkTheme {
    const appColors = AppColors(
      success: Color(0xFFA6DA95), // Macchiato Green
      warning: Color(0xFFEED49F), // Macchiato Yellow
      accent: Color(0xFF8AADF4), // Macchiato Blue
    );

    return ThemeData(
      useMaterial3: true,
      colorScheme: darkColorScheme,
      extensions: [appColors],
      textTheme: textTheme,
      appBarTheme: AppBarTheme(
        backgroundColor: darkColorScheme.surface,
        foregroundColor: darkColorScheme.onSurface,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        titleTextStyle: textTheme.titleMedium?.copyWith(
          color: darkColorScheme.onSurface,
        ),
        systemOverlayStyle: const SystemUiOverlayStyle(
          statusBarColor: Colors.transparent,
          statusBarIconBrightness: Brightness.light,
          statusBarBrightness: Brightness.dark,
        ),
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          backgroundColor: darkColorScheme.primary,
          foregroundColor: darkColorScheme.onPrimary,
          elevation: 0,
          padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(24),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          backgroundColor: darkColorScheme.surface,
          foregroundColor: darkColorScheme.onSurface,
          side: BorderSide(color: darkColorScheme.outline, width: 1),
          padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(24),
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: darkColorScheme.onSurface,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        ),
      ),
      cardTheme: CardThemeData(
        color: darkColorScheme.surfaceContainerHighest,
        margin: EdgeInsets.zero,
        elevation: 0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: darkColorScheme.surfaceContainerHighest,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: darkColorScheme.outline),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: darkColorScheme.outline),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: darkColorScheme.primary),
        ),
        errorBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(8),
          borderSide: BorderSide(color: darkColorScheme.error),
        ),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 16,
          vertical: 12,
        ),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        strokeWidth: 2,
        color: appColors.accent,
        circularTrackColor: darkColorScheme.onSurfaceVariant.withValues(
          alpha: 0.2,
        ),
      ),
      listTileTheme: ListTileThemeData(
        titleTextStyle: textTheme.titleSmall?.copyWith(
          color: darkColorScheme.onSurface,
        ),
        subtitleTextStyle: textTheme.bodyMedium?.copyWith(
          color: darkColorScheme.secondary,
        ),
        iconColor: darkColorScheme.secondary,
        contentPadding: const EdgeInsets.symmetric(horizontal: Grid.twelve),
        minVerticalPadding: Grid.twelve,
        horizontalTitleGap: Grid.twelve,
      ),
      chipTheme: ChipThemeData(
        labelStyle: textTheme.bodySmall?.copyWith(
          color: darkColorScheme.secondary,
        ),
        shape: RoundedRectangleBorder(
          side: BorderSide.none,
          borderRadius: BorderRadius.circular(8),
        ),
        side: BorderSide.none,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        labelPadding: EdgeInsets.zero,
      ),
    );
  }
}
