export const THEMES = [
  { id: "emerald", kind: "light" },
  { id: "light", kind: "light" },
  { id: "mint", kind: "light" },
  { id: "ink", kind: "light" },
  { id: "forest", kind: "light" },
  { id: "sand", kind: "light" },
  { id: "rose", kind: "light" },
  { id: "ocean", kind: "light" },
  { id: "colorful", kind: "light" },
  { id: "contrast", kind: "light" },
  { id: "lavender", kind: "light" },
  { id: "peach", kind: "light" },
  { id: "mac", kind: "light" },
  { id: "notion", kind: "light" },
  { id: "material", kind: "light" },
  { id: "mica", kind: "light" },
  { id: "dark", kind: "dark" },
  { id: "indigo", kind: "dark" },
  { id: "graphite", kind: "dark" },
  { id: "midnight", kind: "dark" },
  { id: "ember", kind: "dark" },
  { id: "pine", kind: "dark" },
  { id: "plum", kind: "dark" },
  { id: "cupertino", kind: "dark" },
  { id: "discord", kind: "dark" },
  { id: "spotify", kind: "dark" },
  { id: "github", kind: "dark" },
  { id: "editor", kind: "dark" },
  { id: "slack", kind: "dark" },
] as const;

export type Theme = (typeof THEMES)[number]["id"];
export type ThemeKind = "light" | "dark";

export const DEFAULT_THEME: Theme = "emerald";

const THEME_IDS: readonly string[] = THEMES.map((t) => t.id);

export function isTheme(v: unknown): v is Theme {
  return typeof v === "string" && THEME_IDS.includes(v);
}

export function applyTheme(theme: Theme) {
  document.documentElement.className = `theme-${theme}`;
}
