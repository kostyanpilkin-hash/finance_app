/** Shift a hex color's channels toward lighter (positive) or darker (negative). */
export function shade(hex: string, percent: number): string {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const n = parseInt(full, 16);
  if (Number.isNaN(n)) return hex;
  const f = (c: number) => Math.min(255, Math.max(0, Math.round(c + (percent / 100) * 255)));
  const r = f((n >> 16) & 0xff);
  const g = f((n >> 8) & 0xff);
  const b = f(n & 0xff);
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
}

/** Readable text color on top of the given hex background. */
export function textOn(hex: string): string {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const n = parseInt(full, 16);
  if (Number.isNaN(n)) return "#ffffff";
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.62 ? "#1f2937" : "#ffffff";
}
