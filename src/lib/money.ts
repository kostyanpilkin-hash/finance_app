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

export function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function nowLocalIso(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${todayIso()}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
