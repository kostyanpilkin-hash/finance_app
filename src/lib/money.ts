export function formatMinor(minor: number | null | undefined, currency = ""): string {
  const v = minor ?? 0;
  const sign = v < 0 ? "-" : "";
  const abs = Math.abs(v);
  const num = `${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
  const grouped = num.replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
  return `${sign}${grouped}${currency ? " " + currency : ""}`;
}

/** "12,5" | "12.50" -> 1250 ; returns null on invalid. */
export function parseMinor(s: string): number | null {
  const t = s.trim().replace(/\s/g, "").replace(",", ".");
  if (!/^\d+(\.\d{1,2})?$/.test(t)) return null;
  const [i, f = ""] = t.split(".");
  return Number(i) * 100 + Number(f.padEnd(2, "0"));
}

/** Local YYYY-MM-DD for an arbitrary Date. */
export function isoOf(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function nowLocalIso(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${todayIso()}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** A row from the `rates_latest` command (rate per BYN scaled by 1e6). */
export interface StoredRate {
  currency: string;
  rateBynScaled: number;
  scale: number;
}

/**
 * Converts `minor` between currencies using already-loaded NBRB rates —
 * same math as the backend `convert_with` (rate_byn_scaled / scale per BYN),
 * rounded to the kopeck. Returns null when a rate is missing.
 */
export function convertMinorLocal(minor: number, from: string, to: string, rates: StoredRate[]): number | null {
  if (from === to) return minor;
  const rate = (c: string): StoredRate | undefined =>
    c === "BYN"
      ? { currency: "BYN", rateBynScaled: 1_000_000, scale: 1 }
      : rates.find((r) => r.currency === c.toUpperCase());
  const f = rate(from);
  const t = rate(to);
  if (!f || !t || f.rateBynScaled <= 0 || t.rateBynScaled <= 0) return null;
  // amount * (f.rate/f.scale) * (t.scale/t.rate), rounded half away from zero.
  const num = BigInt(minor) * BigInt(f.rateBynScaled) * BigInt(t.scale);
  const den = BigInt(f.scale) * BigInt(t.rateBynScaled);
  const half = den / 2n;
  const v = num >= 0n ? (num + half) / den : (num - half) / den;
  return Number(v);
}
