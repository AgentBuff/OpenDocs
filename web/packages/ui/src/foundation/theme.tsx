import {
  createContext,
  useCallback,
  useContext,
  useState,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import { cx } from "../shared/cx.js";

/** Built-in Open Office product skins. Mode is derived from the skin. */
export type ThemeName = "office-light" | "office-dark";
export type ThemeMode = "light" | "dark";
export type ThemeDensity = "compact" | "default" | "comfortable";

export interface ThemeProviderProps extends HTMLAttributes<HTMLDivElement> {
  /** Skin name. */
  theme?: ThemeName;
  /** Optional mode override for a product skin. */
  mode?: ThemeMode;
  /** Density is intentionally independent from color theme. */
  density?: ThemeDensity;
  children: ReactNode;
}

export interface ThemeContextValue {
  theme: ThemeName;
  mode: ThemeMode;
  density: ThemeDensity;
}

/**
 * Product-level theme state.  The renderer remains responsible only for
 * mounting a skin; persistence and switching live in this small runtime
 * boundary so document/artifact models never need to know about UI theme.
 */
export interface ThemeRuntimeValue extends ThemeContextValue {
  setTheme: (theme: ThemeName) => void;
  toggleTheme: () => void;
}

export interface ThemeRuntimeProps extends Omit<ThemeProviderProps, "theme"> {
  /** Theme used when there is no saved preference. */
  initialTheme?: ThemeName;
  /** Set to null to make the runtime session-only. */
  storageKey?: string | null;
}

const ThemeContext = createContext<ThemeContextValue>({
  theme: "office-light",
  mode: "light",
  density: "default",
});

const ThemeRuntimeContext = createContext<ThemeRuntimeValue | null>(null);

function themeMode(theme: ThemeName): ThemeMode {
  return theme === "office-dark" ? "dark" : "light";
}

/** Provides a scoped theme and density through DOM attributes and context. */
export function ThemeProvider({
  theme = "office-light",
  mode,
  density = "default",
  className,
  children,
  ...props
}: ThemeProviderProps) {
  const resolvedMode = mode ?? themeMode(theme);
  const value: ThemeContextValue = { theme, mode: resolvedMode, density };

  return (
    <ThemeContext.Provider value={value}>
      <div
        {...props}
        className={cx("oo-theme-root", `oo-theme--${theme}`, `oo-theme--${resolvedMode}`, className)}
        data-theme={resolvedMode}
        data-density={density}
      >
        {children}
      </div>
    </ThemeContext.Provider>
  );
}

function isThemeName(value: string | null): value is ThemeName {
  return value === "office-light" || value === "office-dark";
}

function readStoredTheme(storageKey: string | null | undefined, fallback: ThemeName): ThemeName {
  if (!storageKey || typeof window === "undefined") return fallback;
  try {
    const stored = window.localStorage.getItem(storageKey);
    return isThemeName(stored) ? stored : fallback;
  } catch {
    return fallback;
  }
}

/**
 * Stateful theme boundary for an application shell.  It intentionally wraps
 * the stateless ThemeProvider instead of making every component own theme
 * state.  A storage key is opt-in by contract and can be disabled for
 * embedded consumers with `null`.
 */
export function ThemeRuntime({
  initialTheme = "office-light",
  storageKey = "open-office.theme",
  children,
  ...providerProps
}: ThemeRuntimeProps) {
  const [theme, setThemeState] = useState<ThemeName>(() => readStoredTheme(storageKey, initialTheme));
  const setTheme = useCallback((next: ThemeName) => {
    setThemeState(next);
    if (!storageKey || typeof window === "undefined") return;
    try {
      window.localStorage.setItem(storageKey, next);
    } catch {
      // Storage can be unavailable in private/embedded contexts; the
      // in-memory runtime remains fully functional.
    }
  }, [storageKey]);
  const toggleTheme = useCallback(() => {
    setTheme(theme === "office-dark" ? "office-light" : "office-dark");
  }, [setTheme, theme]);
  const mode = themeMode(theme);
  const value: ThemeRuntimeValue = { theme, mode, density: providerProps.density ?? "default", setTheme, toggleTheme };

  return (
    <ThemeProvider {...providerProps} theme={theme}>
      <ThemeRuntimeContext.Provider value={value}>{children}</ThemeRuntimeContext.Provider>
    </ThemeProvider>
  );
}

export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext);
}

/** Access the application shell's switchable theme state. */
export function useThemeRuntime(): ThemeRuntimeValue {
  const value = useContext(ThemeRuntimeContext);
  if (!value) throw new Error("useThemeRuntime must be used inside ThemeRuntime");
  return value;
}
