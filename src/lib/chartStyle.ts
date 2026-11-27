/** Shared axis look: quiet ticks, no tick marks or axis lines. */
export const AXIS = {
  fontSize: 11,
  tickLine: false,
  axisLine: false,
  stroke: "var(--muted-foreground)",
} as const;

/** Shared horizontal grid (pass to CartesianGrid). */
export const GRID = { strokeDasharray: "3 3", stroke: "var(--border)", vertical: false } as const;

/** Column highlight under the pointer for clickable bar charts. */
export const HOVER_CURSOR = { fill: "var(--muted)", opacity: 0.6 } as const;

/** Compact money ticks for a minor-unit axis: 1,2 тыс., 850, 3 млн. */
export function compactTick(lang: string) {
  const nf = new Intl.NumberFormat(lang, { notation: "compact", maximumFractionDigits: 1 });
  return (minor: number) => nf.format(minor / 100);
}
