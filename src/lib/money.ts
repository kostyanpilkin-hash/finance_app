import i18n from "@/i18n";

/** ru/be use thin spaces for thousands and a decimal comma; en keeps 1,234.56.
 * The minus is typographic (−), and every sum is locale-aware. */
export function formatMinor(minor: number | null | undefined, currency = ""): string {
  const v = minor != null && Number.isFinite(minor) ? minor : 0;
  const sign = v < 0 ? "\u2212" : "";
  const abs = Math.abs(v);
  const locale = (i18n.language ?? "ru").startsWith("en") ? "en-US" : "ru-RU";
  const num = new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(abs / 100);
  return `${sign}${num}${currency ? " " + currency : ""}`;
}

/** "12,5" | "12.50" | "−12.50" -> 1250 | −1250 ; returns null on invalid. */
export function parseMinor(s: string): number | null {
  let t = s.trim().replace(/\s/g, "").replace(",", ".");
  const neg = t.startsWith("-") || t.startsWith("−");
  if (neg) t = t.slice(1);
  if (!/^\d+(\.\d{1,2})?$/.test(t)) return null;
  const [i, f = ""] = t.split(".");
  const v = Number(i) * 100 + Number(f.padEnd(2, "0"));
  return neg ? -v : v;
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

/** "2026-11-15" → "15 ноября" / "Nov 15"; year added only when it differs from the current one. */
export function formatDateHuman(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(`${iso.slice(0, 10)}T00:00:00`);
  if (Number.isNaN(d.getTime())) return String(iso);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(i18n.language ?? "ru", {
    day: "numeric", month: "long", ...(sameYear ? {} : { year: "numeric" }),
  });
}

/** "2026-11-01"…"2026-11-30" → "1 – 30 ноября" / "November 1–30" via Intl.formatRange. */
export function formatPeriodHuman(from: string, to: string): string {
  const a = new Date(`${from}T00:00:00`), b = new Date(`${to}T00:00:00`);
  if (Number.isNaN(a.getTime()) || Number.isNaN(b.getTime())) return `${from} — ${to}`;
  const sameYear = a.getFullYear() === new Date().getFullYear();
  const opts: Intl.DateTimeFormatOptions = {
    day: "numeric", month: "long", ...(sameYear ? {} : { year: "numeric" }),
  };
  const f = new Intl.DateTimeFormat(i18n.language ?? "ru", opts) as Intl.DateTimeFormat & {
    formatRange?: (x: Date, y: Date) => string;
  };
  return f.formatRange ? f.formatRange(a, b) : `${formatDateHuman(from)} — ${formatDateHuman(to)}`;
}

/** Day-group header like "Сегодня, 15 ноября" / "Вчера, …" / "15 ноября". */
export function dayLabel(isoDay: string, t: (k: string) => string): string {
  const dm = formatDateHuman(isoDay);
  const y = new Date();
  y.setDate(y.getDate() - 1);
  if (isoDay === todayIso()) return `${t("ui.today")}, ${dm}`;
  if (isoDay === isoOf(y)) return `${t("ui.yesterday")}, ${dm}`;
  return dm;
}

/** A row from the `rates_latest` command (rate per BYN scaled by 1e6). */
export interface StoredRate {
  currency: string;
  rateBynScaled: number;
  scale: number;
}

/** Rate pair = (rateBynScaled, scale), same formula as Rust convert_with:
 *  amount * fRate/fScale * tScale/tRate, rounded half away from zero. */
export function convertMinor(
  minor: number,
  from: { rate: number; scale: number },
  to: { rate: number; scale: number },
): number | null {
  if (from.rate <= 0 || to.rate <= 0 || from.scale <= 0 || to.scale <= 0) return null;
  const num = BigInt(minor) * BigInt(from.rate) * BigInt(to.scale);
  const den = BigInt(from.scale) * BigInt(to.rate);
  const half = den / 2n;
  const v = num >= 0n ? (num + half) / den : (num - half) / den;
  return Number(v);
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
  if (!f || !t) return null;
  if (!Number.isFinite(f.rateBynScaled) || !Number.isFinite(f.scale) ||
      !Number.isFinite(t.rateBynScaled) || !Number.isFinite(t.scale)) return null;
  return convertMinor(minor, { rate: f.rateBynScaled, scale: f.scale }, { rate: t.rateBynScaled, scale: t.scale });
}
