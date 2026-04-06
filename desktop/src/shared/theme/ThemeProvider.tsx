import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { createAdaptiveTheme, themeToShadcnVarMap } from "./adaptive-theme";
import {
  SYNTAX_THEMES,
  type SyntaxThemeName,
  extractThemeInfo,
  loadThemeData,
} from "./theme-loader";

const STORAGE_KEY = "sprout-theme";
const CACHE_KEY = "sprout-theme-cache";

type ThemeContextValue = {
  themeName: string;
  isDark: boolean;
  isLoading: boolean;
  setTheme: (name: string) => void;
};

type ThemeProviderProps = {
  children: ReactNode;
  defaultTheme?: SyntaxThemeName;
};

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function isValidThemeName(name: string): name is SyntaxThemeName {
  return (SYNTAX_THEMES as readonly string[]).includes(name);
}

/** Read stored theme, migrating legacy "light"/"dark"/"system" values. */
function readStoredTheme(fallback: SyntaxThemeName): SyntaxThemeName {
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (!stored) return fallback;

  // Migrate legacy values
  if (stored === "light") return "catppuccin-latte";
  if (stored === "dark" || stored === "system") return "catppuccin-macchiato";

  return isValidThemeName(stored) ? stored : fallback;
}

/** Apply cached CSS vars synchronously to prevent FOUC. */
function applyCachedVars(): string | null {
  try {
    const cached = window.localStorage.getItem(CACHE_KEY);
    if (!cached) return null;
    const { themeName, vars, isDark } = JSON.parse(cached);
    const root = document.documentElement;
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value as string);
    }
    root.classList.remove("light", "dark");
    root.classList.add(isDark ? "dark" : "light");
    return themeName;
  } catch {
    return null;
  }
}

/** Apply a theme: load data, create adaptive palette, set CSS vars. */
async function applyTheme(name: SyntaxThemeName): Promise<{ isDark: boolean }> {
  const themeData = await loadThemeData(name);
  const info = extractThemeInfo(name, themeData);
  const adaptive = createAdaptiveTheme(info.bg, info.fg, info.comment, {
    added: info.added,
    deleted: info.deleted,
    modified: info.modified,
  });
  const vars = themeToShadcnVarMap(adaptive);

  const root = document.documentElement;
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }

  root.classList.remove("light", "dark");
  root.classList.add(adaptive.isDark ? "dark" : "light");

  // Cache for FOUC prevention
  try {
    window.localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ themeName: name, vars, isDark: adaptive.isDark }),
    );
  } catch {
    // Storage full — non-critical
  }

  return { isDark: adaptive.isDark };
}

export function ThemeProvider({
  children,
  defaultTheme = "catppuccin-macchiato",
}: ThemeProviderProps) {
  // Apply cached vars synchronously before first render
  const [themeName, setThemeName] = useState<string>(() => {
    const cached = applyCachedVars();
    return cached ?? readStoredTheme(defaultTheme);
  });
  const [isDark, setIsDark] = useState<boolean>(() => {
    return document.documentElement.classList.contains("dark");
  });
  const [isLoading, setIsLoading] = useState(true);
  const loadingRef = useRef<string | null>(null);

  // Load and apply theme
  useEffect(() => {
    if (!isValidThemeName(themeName)) return;

    // Track which theme we're loading to avoid race conditions
    const thisTheme = themeName;
    loadingRef.current = thisTheme;
    setIsLoading(true);

    applyTheme(themeName).then(({ isDark: dark }) => {
      // Only update if this is still the theme we want
      if (loadingRef.current === thisTheme) {
        setIsDark(dark);
        setIsLoading(false);
      }
    });
  }, [themeName]);

  const setTheme = useCallback((name: string) => {
    if (!isValidThemeName(name)) return;
    setThemeName(name);
    window.localStorage.setItem(STORAGE_KEY, name);
  }, []);

  const value: ThemeContextValue = {
    themeName,
    isDark,
    isLoading,
    setTheme,
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
