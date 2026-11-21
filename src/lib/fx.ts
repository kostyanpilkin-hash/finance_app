import type { RateKind, RateSource } from "./types";

/** Above this |deviation| from the bank rate the actual rate is highlighted. */
export const DEVIATION_WARN_PCT = 1;

const isByn = (c: string) => c.toUpperCase() === "BYN";

/** BYN per 1 unit of the foreign currency (×1e6) implied by two amounts —
 *  same as Rust `fx::effective_rate`. Null unless exactly one side is BYN. */
export function effectiveRate(from: string, fromMinor: number, to: string, toMinor: number): number | null {
  if (!(fromMinor > 0) || !(toMinor > 0)) return null;
  let byn: number, foreign: number;
  if (isByn(from) && !isByn(to)) [byn, foreign] = [fromMinor, toMinor];
  else if (!isByn(from) && isByn(to)) [byn, foreign] = [toMinor, fromMinor];
  else return null;
  return Math.round((byn * 1_000_000) / foreign);
}

/** Percent difference of `actual` from `reference`, one decimal. */
export function deviationPct(actualScaled: number, refScaled: number): number {
  if (!(refScaled > 0)) return 0;
  return Math.round(((actualScaled - refScaled) / refScaled) * 1000) / 10;
}

/** Rate source after the user typed `typed` into a converted-amount field:
 *  still the filled kind while it matches the filled amount, else manual. */
export function nextRateSource(filled: { amount: number; kind: RateKind } | null, typed: number | null): RateSource | null {
  if (!typed) return null;
  return filled && filled.amount === typed ? filled.kind : "manual";
}

/** Units a rate is quoted for (RUB per 100, PLN per 10). */
export function rateUnits(currency: string): number {
  const c = currency.toUpperCase();
  return c === "RUB" ? 100 : c === "PLN" ? 10 : 1;
}

/** Scaled per-unit rate → "3,4753" for `rateUnits(currency)` units. */
export function formatRate(scaled: number, currency: string, lang = "ru"): string {
  return new Intl.NumberFormat(lang, { minimumFractionDigits: 4, maximumFractionDigits: 4 })
    .format((scaled * rateUnits(currency)) / 1_000_000);
}

export const RATE_KINDS: RateKind[] = ["bsb_app", "bsb_card", "nbrb"];
export const rateKindKey = (k: RateSource): string =>
  ({ bsb_app: "ui.fx.bsbApp", bsb_card: "ui.fx.bsbCard", nbrb: "ui.fx.nbrb", manual: "ui.fx.manual" })[k];
