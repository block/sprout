import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { createThemeVars, hexToHsl } from "./adaptive-theme";
import {
  SYNTAX_THEMES,
  isLightTheme,
  type SyntaxThemeName,
  extractThemeInfo,
  loadThemeData,
} from "./theme-loader";

const STORAGE_KEY = "sprout-theme";
const CACHE_KEY = "sprout-theme-cache";
const ACCENT_KEY = "sprout-accent-color";
const CACHE_VERSION = 2;
const DEFAULT_THEME: SyntaxThemeName = "houston";
const DEFAULT_ACCENT = "#3b82f6";

export const ACCENT_COLORS = [
  { name: "Blue", value: "#3b82f6" },
  { name: "Cyan", value: "#06b6d4" },
  { name: "Green", value: "#22c55e" },
  { name: "Orange", value: "#f97316" },
  { name: "Red", value: "#ef4444" },
  { name: "Pink", value: "#ec4899" },
  { name: "Lilac", value: "#c0a2f1" },
  { name: "Purple", value: "#a855f7" },
  { name: "Indigo", value: "#6366f1" },
] as const;

type ThemeContextValue = {
  themeName: SyntaxThemeName;
  isDark: boolean;
  isLoading: boolean;
  accentColor: string;
  setTheme: (name: string) => void;
  setAccentColor: (color: string) => void;
};

type ThemeProviderProps = {
  children: ReactNode;
};

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function isValidThemeName(name: string): name is SyntaxThemeName {
  return (SYNTAX_THEMES as readonly string[]).includes(name);
}

/** Read the stored explicit theme, migrating legacy appearance values. */
function readStoredTheme(): SyntaxThemeName {
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (!stored) {
    return DEFAULT_THEME;
  }

  if (stored === "light") {
    return "catppuccin-latte";
  }

  if (stored === "dark" || stored === "system") {
    return DEFAULT_THEME;
  }

  return isValidThemeName(stored) ? stored : DEFAULT_THEME;
}

function getContrastColor(hex: string): string {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i.exec(hex);
  if (!m) return "#ffffff";
  const r = parseInt(m[1], 16);
  const g = parseInt(m[2], 16);
  const b = parseInt(m[3], 16);
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.5 ? "#000000" : "#ffffff";
}

function applyAccentColor(hex: string) {
  const root = document.documentElement;
  const accentHsl = hexToHsl(hex);
  const fgHsl = hexToHsl(getContrastColor(hex));
  root.style.setProperty("--primary", accentHsl);
  root.style.setProperty("--primary-foreground", fgHsl);
  root.style.setProperty("--sidebar-primary", accentHsl);
  root.style.setProperty("--sidebar-primary-foreground", fgHsl);
}

function applyThemeClass(themeName: SyntaxThemeName) {
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  root.classList.add(isLightTheme(themeName) ? "light" : "dark");
}

/** Apply cached CSS vars synchronously to prevent FOUC. */
function applyCachedVars(): SyntaxThemeName | null {
  try {
    const cached = window.localStorage.getItem(CACHE_KEY);
    if (!cached) return null;

    const {
      version,
      themeName,
      vars,
      isDark,
    }: {
      version?: number;
      themeName?: string;
      vars?: Record<string, string>;
      isDark?: boolean;
    } = JSON.parse(cached);

    if (
      version !== CACHE_VERSION ||
      !themeName ||
      !isValidThemeName(themeName) ||
      !vars ||
      typeof isDark !== "boolean"
    ) {
      return null;
    }

    const root = document.documentElement;
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value);
    }
    root.classList.remove("light", "dark");
    root.classList.add(isDark ? "dark" : "light");
    applyAccentColor(window.localStorage.getItem(ACCENT_KEY) ?? DEFAULT_ACCENT);
    return themeName;
  } catch {
    return null;
  }
}

/** Apply a theme: load data, derive CSS vars, set them on :root. */
async function applyTheme(name: SyntaxThemeName): Promise<{ isDark: boolean }> {
  const themeData = await loadThemeData(name);
  const info = extractThemeInfo(name, themeData);
  const { isDark, vars } = createThemeVars(info.bg, info.fg, info.comment, {
    added: info.added,
    deleted: info.deleted,
    modified: info.modified,
  });

  const root = document.documentElement;
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }

  root.classList.remove("light", "dark");
  root.classList.add(isDark ? "dark" : "light");

  // Cache for FOUC prevention
  try {
    window.localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ version: CACHE_VERSION, themeName: name, vars, isDark }),
    );
  } catch {
    // Storage full — non-critical
  }

  return { isDark };
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  // Apply cached vars synchronously before first render when available.
  const [themeName, setThemeName] = useState<SyntaxThemeName>(() => {
    const cachedTheme = applyCachedVars();
    if (cachedTheme) {
      return cachedTheme;
    }

    const storedTheme = readStoredTheme();
    applyThemeClass(storedTheme);
    return storedTheme;
  });
  const [isDark, setIsDark] = useState<boolean>(() =>
    document.documentElement.classList.contains("dark"),
  );
  const [isLoading, setIsLoading] = useState(true);
  const [accentColor, setAccentColorState] = useState<string>(() => {
    return window.localStorage.getItem(ACCENT_KEY) ?? DEFAULT_ACCENT;
  });
  const loadingRef = useRef<SyntaxThemeName | null>(null);

  // Load and apply the selected theme.
  useEffect(() => {
    const thisTheme = themeName;
    loadingRef.current = thisTheme;
    setIsLoading(true);

    applyTheme(themeName)
      .then(({ isDark: dark }) => {
        if (loadingRef.current !== thisTheme) return;
        setIsDark(dark);
        setIsLoading(false);
        applyAccentColor(
          window.localStorage.getItem(ACCENT_KEY) ?? DEFAULT_ACCENT,
        );
        window.localStorage.setItem(STORAGE_KEY, thisTheme);
      })
      .catch(() => {
        if (loadingRef.current !== thisTheme) return;
        setIsLoading(false);
      });
  }, [themeName]);

  // Apply accent color changes on top of the selected theme.
  useEffect(() => {
    applyAccentColor(accentColor);
  }, [accentColor]);

  const setTheme = useCallback((name: string) => {
    if (!isValidThemeName(name)) {
      return;
    }

    setThemeName(name);
    window.localStorage.setItem(STORAGE_KEY, name);
  }, []);

  const setAccentColor = useCallback((color: string) => {
    window.localStorage.setItem(ACCENT_KEY, color);
    setAccentColorState(color);
  }, []);

  const value: ThemeContextValue = {
    themeName,
    isDark,
    isLoading,
    accentColor,
    setTheme,
    setAccentColor,
  };

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
