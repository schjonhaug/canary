export const THEME_STORAGE_KEY = "canary-theme"

export const THEME_OPTIONS = ["system", "light", "dark"] as const

export type ThemePreference = (typeof THEME_OPTIONS)[number]
export type ResolvedTheme = Exclude<ThemePreference, "system">

export function isThemePreference(value: string | null | undefined): value is ThemePreference {
  return value !== null && value !== undefined && THEME_OPTIONS.includes(value as ThemePreference)
}

export function resolveTheme(preference: ThemePreference, prefersDark: boolean): ResolvedTheme {
  if (preference === "system") {
    return prefersDark ? "dark" : "light"
  }

  return preference
}

export function applyTheme(theme: ResolvedTheme): void {
  const root = document.documentElement
  root.classList.toggle("dark", theme === "dark")
  root.style.colorScheme = theme
}

export function getThemeInitializationScript(): string {
  return `
    (function() {
      var storageKey = '${THEME_STORAGE_KEY}';
      var root = document.documentElement;
      var supportsMatchMedia = typeof window.matchMedia === 'function';
      var stored = null;

      try {
        stored = window.localStorage.getItem(storageKey);
      } catch (error) {}

      var preference = stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system';
      var resolved = preference === 'system'
        ? (supportsMatchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
        : preference;

      root.classList.toggle('dark', resolved === 'dark');
      root.style.colorScheme = resolved;
    })();
  `
}
