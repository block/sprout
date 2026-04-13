import 'package:flutter/material.dart';

// Catppuccin Latte (mauve accent) — matches Sprout desktop light theme
const lightColorScheme = ColorScheme(
  brightness: Brightness.light,
  primary: Color(0xFF8839EF),       // Latte Mauve
  onPrimary: Color(0xFFEFF1F5),     // Latte Base
  primaryContainer: Color(0xFFE6E9EF), // Latte Mantle
  onPrimaryContainer: Color(0xFF4C4F69), // Latte Text
  secondary: Color(0xFF6C6F85),     // Latte Subtext0
  onSecondary: Color(0xFFFFFFFF),
  secondaryContainer: Color(0xFFCCD0DA), // Latte Surface1
  onSecondaryContainer: Color(0xFF4C4F69), // Latte Text
  tertiary: Color(0xFF1E66F5),      // Latte Blue
  onTertiary: Color(0xFFFFFFFF),
  tertiaryContainer: Color(0xFFBCC0CC), // Latte Surface2
  onTertiaryContainer: Color(0xFF1E66F5), // Latte Blue
  error: Color(0xFFD20F39),         // Latte Red
  onError: Color(0xFFFFFFFF),
  errorContainer: Color(0xFFFDD8E0),
  onErrorContainer: Color(0xFFD20F39),
  surface: Color(0xFFEFF1F5),       // Latte Base
  onSurface: Color(0xFF4C4F69),     // Latte Text
  onSurfaceVariant: Color(0xFF6C6F85), // Latte Subtext0
  outline: Color(0xFFBCC0CC),       // Latte Surface2
  outlineVariant: Color(0xFFCCD0DA), // Latte Surface1
  inverseSurface: Color(0xFF4C4F69), // Latte Text
  onInverseSurface: Color(0xFFEFF1F5), // Latte Base
  inversePrimary: Color(0xFFC6A0F6), // Macchiato Mauve
  shadow: Color(0xFF000000),
  scrim: Color(0xFF000000),
  surfaceTint: Color(0xFF8839EF),   // Latte Mauve
  surfaceContainerHighest: Color(0xFFFFFFFF),
);

// Catppuccin Macchiato (mauve accent) — matches Sprout desktop dark theme
const darkColorScheme = ColorScheme(
  brightness: Brightness.dark,
  primary: Color(0xFFC6A0F6),       // Macchiato Mauve
  onPrimary: Color(0xFF24273A),     // Macchiato Base
  primaryContainer: Color(0xFF363A4F), // Macchiato Surface0
  onPrimaryContainer: Color(0xFFCAD3F5), // Macchiato Text
  secondary: Color(0xFFA5ADCB),     // Macchiato Subtext0
  onSecondary: Color(0xFF24273A),   // Macchiato Base
  secondaryContainer: Color(0xFF494D64), // Macchiato Surface1
  onSecondaryContainer: Color(0xFFCAD3F5), // Macchiato Text
  tertiary: Color(0xFF8AADF4),      // Macchiato Blue
  onTertiary: Color(0xFF24273A),    // Macchiato Base
  tertiaryContainer: Color(0xFF363A4F), // Macchiato Surface0
  onTertiaryContainer: Color(0xFF8AADF4), // Macchiato Blue
  error: Color(0xFFED8796),         // Macchiato Red
  onError: Color(0xFF24273A),       // Macchiato Base
  errorContainer: Color(0xFF3D2030),
  onErrorContainer: Color(0xFFED8796),
  surface: Color(0xFF24273A),       // Macchiato Base
  onSurface: Color(0xFFCAD3F5),     // Macchiato Text
  onSurfaceVariant: Color(0xFFA5ADCB), // Macchiato Subtext0
  outline: Color(0xFF494D64),       // Macchiato Surface1
  outlineVariant: Color(0xFF363A4F), // Macchiato Surface0
  inverseSurface: Color(0xFFCAD3F5), // Macchiato Text
  onInverseSurface: Color(0xFF24273A), // Macchiato Base
  inversePrimary: Color(0xFF8839EF), // Latte Mauve
  shadow: Color(0xFF000000),
  scrim: Color(0xFF000000),
  surfaceTint: Color(0xFFC6A0F6),   // Macchiato Mauve
  surfaceContainerHighest: Color(0xFF1E2030), // Macchiato Mantle
);
