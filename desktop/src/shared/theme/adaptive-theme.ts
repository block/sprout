/**
 * Adaptive Theme Engine
 *
 * Derives a full UI palette from a syntax theme's key colors (bg, fg, comment, git).
 * Detects light vs dark from background luminance and adjusts accordingly.
 *
 * Ported from builderbot/apps/staged/src/lib/theme.ts, trimmed for sprout's needs.
 * Adds hexToHslComponents() + themeToShadcnVarMap() for Tailwind/shadcn compatibility.
 */

// =============================================================================
// Theme Interface (trimmed for sprout)
// =============================================================================

export interface Theme {
  isDark: boolean;

  bg: {
    primary: string; // Main background (same as syntax theme)
    chrome: string; // Sidebar chrome
    deepest: string; // Deepest level
    elevated: string; // Floating elements
    hover: string; // Hover states
  };

  border: {
    subtle: string;
    muted: string;
    emphasis: string;
  };

  text: {
    primary: string;
    muted: string; // Subdued text (comment color)
    faint: string;
    accent: string;
  };

  status: {
    modified: string;
    added: string;
    deleted: string;
    renamed: string;
    untracked: string;
  };

  ui: {
    accent: string;
    accentHover: string;
    danger: string;
    dangerBg: string;
    warning: string;
    warningBg: string;
    selection: string;
  };

  scrollbar: {
    thumb: string;
    thumbHover: string;
  };

  shadow: {
    overlay: string;
    elevated: string;
  };
}

// =============================================================================
// Color Utilities
// =============================================================================

interface RGB {
  r: number;
  g: number;
  b: number;
}

function hexToRgb(hex: string): RGB {
  const long = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})?$/i.exec(
    hex,
  );
  if (long) {
    return {
      r: parseInt(long[1], 16),
      g: parseInt(long[2], 16),
      b: parseInt(long[3], 16),
    };
  }

  const short = /^#?([a-f\d])([a-f\d])([a-f\d])([a-f\d])?$/i.exec(hex);
  if (short) {
    return {
      r: parseInt(short[1] + short[1], 16),
      g: parseInt(short[2] + short[2], 16),
      b: parseInt(short[3] + short[3], 16),
    };
  }

  return { r: 128, g: 128, b: 128 };
}

function rgbToHex({ r, g, b }: RGB): string {
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n)));
  return `#${[r, g, b].map((c) => clamp(c).toString(16).padStart(2, "0")).join("")}`;
}

export function luminance(hex: string): number {
  const { r, g, b } = hexToRgb(hex);
  const [rs, gs, bs] = [r, g, b].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
}

function mix(hex1: string, hex2: string, factor: number): string {
  const c1 = hexToRgb(hex1);
  const c2 = hexToRgb(hex2);
  return rgbToHex({
    r: c1.r + (c2.r - c1.r) * factor,
    g: c1.g + (c2.g - c1.g) * factor,
    b: c1.b + (c2.b - c1.b) * factor,
  });
}

function adjust(hex: string, amount: number): string {
  const target = amount > 0 ? "#ffffff" : "#000000";
  return mix(hex, target, Math.abs(amount));
}

function overlay(hex: string, alpha: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

// =============================================================================
// Chrome Color Calculation
// =============================================================================

const CONTRAST_VALUE = 0.035;
const CONTRAST_OFFSET = 0.0135;

function calculateLumDiff(bgLum: number): number {
  return CONTRAST_VALUE * Math.log(1 + (bgLum + CONTRAST_OFFSET) * 10);
}

function findColorWithLuminance(baseColor: string, targetLum: number): string {
  const baseLum = luminance(baseColor);
  if (Math.abs(baseLum - targetLum) < 0.001) return baseColor;

  const target = targetLum < baseLum ? "#000000" : "#ffffff";
  let lo = 0;
  let hi = 1;

  for (let i = 0; i < 20; i++) {
    const mid = (lo + hi) / 2;
    const testLum = luminance(mix(baseColor, target, mid));
    const diff = testLum - targetLum;

    if (Math.abs(diff) < 0.001) break;

    if (target === "#000000") {
      if (testLum > targetLum) lo = mid;
      else hi = mid;
    } else {
      if (testLum < targetLum) lo = mid;
      else hi = mid;
    }
  }
  return mix(baseColor, target, (lo + hi) / 2);
}

interface ChromeColors {
  chrome: string;
  deepest: string;
  primary: string;
}

function calculateChromeColors(syntaxBg: string): ChromeColors {
  const bgLum = luminance(syntaxBg);
  const lumDiff = calculateLumDiff(bgLum);
  const targetChromeLum = bgLum - lumDiff;
  const deepestLumDiff = lumDiff * 2;
  const targetDeepestLum = bgLum - deepestLumDiff;

  if (targetChromeLum >= 0) {
    return {
      chrome: findColorWithLuminance(syntaxBg, targetChromeLum),
      deepest: findColorWithLuminance(syntaxBg, Math.max(0, targetDeepestLum)),
      primary: syntaxBg,
    };
  }

  return {
    chrome: findColorWithLuminance(syntaxBg, 0),
    deepest: findColorWithLuminance(syntaxBg, 0),
    primary: findColorWithLuminance(syntaxBg, lumDiff),
  };
}

// =============================================================================
// Adaptive Theme Generator
// =============================================================================

export interface ThemeGitColors {
  added: string | null;
  deleted: string | null;
  modified: string | null;
}

export function createAdaptiveTheme(
  syntaxBg: string,
  syntaxFg: string,
  syntaxComment: string,
  gitColors?: ThemeGitColors,
): Theme {
  const isDark = luminance(syntaxBg) < 0.5;

  const {
    chrome: chromeColor,
    deepest: deepestColor,
    primary: primaryBg,
  } = calculateChromeColors(syntaxBg);

  const dir = isDark ? 1 : -1;
  const elevate = (amount: number) => adjust(primaryBg, dir * amount);

  const fallbackBlue = isDark ? "#58a6ff" : "#0969da";
  const fallbackGreen = isDark ? "#3fb950" : "#1a7f37";
  const fallbackRed = isDark ? "#f85149" : "#cf222e";
  const fallbackOrange = isDark ? "#d29922" : "#9a6700";

  const accentGreen = gitColors?.added ?? fallbackGreen;
  const accentRed = gitColors?.deleted ?? fallbackRed;
  const accentBlue = gitColors?.modified ?? fallbackBlue;
  const accentOrange = fallbackOrange;

  const borderBase = mix(primaryBg, syntaxFg, isDark ? 0.15 : 0.12);

  return {
    isDark,

    bg: {
      primary: primaryBg,
      chrome: chromeColor,
      deepest: deepestColor,
      elevated: elevate(0.08),
      hover: elevate(0.06),
    },

    border: {
      subtle: mix(primaryBg, syntaxFg, 0.08),
      muted: borderBase,
      emphasis: mix(primaryBg, syntaxFg, isDark ? 0.25 : 0.2),
    },

    text: {
      primary: syntaxFg,
      muted: syntaxComment,
      faint: mix(primaryBg, syntaxComment, 0.5),
      accent: accentBlue,
    },

    status: {
      modified: accentOrange,
      added: accentGreen,
      deleted: accentRed,
      renamed: accentBlue,
      untracked: syntaxComment,
    },

    ui: {
      accent: accentGreen,
      accentHover: isDark
        ? adjust(accentGreen, -0.15)
        : adjust(accentGreen, 0.15),
      danger: accentRed,
      dangerBg: overlay(accentRed, isDark ? 0.1 : 0.08),
      warning: accentOrange,
      warningBg: overlay(accentOrange, isDark ? 0.1 : 0.08),
      selection: overlay(syntaxFg, isDark ? 0.08 : 0.1),
    },

    scrollbar: {
      thumb: borderBase,
      thumbHover: mix(primaryBg, syntaxFg, 0.25),
    },

    shadow: {
      overlay: isDark ? "rgba(0, 0, 0, 0.6)" : "rgba(0, 0, 0, 0.4)",
      elevated: isDark
        ? `0 8px 24px ${overlay("#000000", 0.4)}`
        : `0 8px 24px ${overlay("#000000", 0.15)}`,
    },
  };
}

// =============================================================================
// HSL Bridge for Tailwind/shadcn
// =============================================================================

/**
 * Convert a hex color to HSL component format: "H S% L%"
 * This is what Tailwind expects inside hsl() wrappers.
 */
export function hexToHslComponents(hex: string): string {
  const { r, g, b } = hexToRgb(hex);
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;

  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;

  if (max === min) {
    return `0 0% ${(l * 100).toFixed(1)}%`;
  }

  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

  let h: number;
  if (max === rn) {
    h = ((gn - bn) / d + (gn < bn ? 6 : 0)) / 6;
  } else if (max === gn) {
    h = ((bn - rn) / d + 2) / 6;
  } else {
    h = ((rn - gn) / d + 4) / 6;
  }

  return `${(h * 360).toFixed(1)} ${(s * 100).toFixed(2)}% ${(l * 100).toFixed(1)}%`;
}

/**
 * Map a Theme to shadcn CSS variable names with HSL component values.
 * These get applied via document.documentElement.style.setProperty().
 */
export function themeToShadcnVarMap(t: Theme): Record<string, string> {
  const h = hexToHslComponents;

  // For foreground-on-primary, use the bg as the contrast color
  const primaryFg = h(t.bg.primary);
  // For accent foreground, use primary text
  const accentFg = h(t.text.primary);

  return {
    // Backgrounds
    "--background": h(t.bg.primary),
    "--card": h(t.bg.primary),
    "--popover": h(t.bg.elevated),
    "--muted": h(t.bg.hover),
    "--accent": h(t.bg.hover),
    "--secondary": h(t.bg.hover),

    // Foregrounds
    "--foreground": h(t.text.primary),
    "--card-foreground": h(t.text.primary),
    "--popover-foreground": h(t.text.primary),
    "--muted-foreground": h(t.text.muted),
    "--accent-foreground": accentFg,
    "--secondary-foreground": accentFg,

    // Primary interactive
    "--primary": h(t.ui.accent),
    "--primary-foreground": primaryFg,

    // Destructive
    "--destructive": h(t.ui.danger),
    "--destructive-foreground": primaryFg,

    // Borders
    "--border": h(t.border.muted),
    "--input": h(t.border.muted),
    "--ring": h(t.text.primary),

    // Sidebar
    "--sidebar-background": h(t.bg.chrome),
    "--sidebar-foreground": h(t.text.primary),
    "--sidebar-primary": h(t.ui.accent),
    "--sidebar-primary-foreground": primaryFg,
    "--sidebar-accent": h(t.bg.primary),
    "--sidebar-accent-foreground": accentFg,
    "--sidebar-border": h(t.border.muted),
    "--sidebar-ring": h(t.border.muted),

    // Status colors (hex, not HSL — used directly via var())
    "--status-added": t.status.added,
    "--status-deleted": t.status.deleted,
    "--status-modified": t.status.modified,

    // Warning
    "--ui-warning": t.ui.warning,
    "--ui-warning-bg": t.ui.warningBg,
  };
}
