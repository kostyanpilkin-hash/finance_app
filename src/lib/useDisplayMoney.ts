import { useCallback, useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useSettings } from "@/lib/queries";
import { convertMinor, todayIso } from "@/lib/money";

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

/**
 * Display currency + conversion of any minor-unit amount into it.
 * Returns null from toDisplay when a rate is missing.
 */
export function useDisplayMoney() {
  const settings = useSettings();
  const rates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<RateRow>() });
  const displayCurrency = settings.data?.display_currency || "BYN";

  const toDisplay = useCallback(
    (minor: number, cur: string): number | null => {
      const pair = (c: string) =>
        c === "BYN" ? { rate: 1_000_000, scale: 1 } : (() => {
          const r = rates.data?.find((x) => x.currency === c);
          return r ? { rate: r.rateBynScaled, scale: r.scale || 1 } : null;
        })();
      const f = pair(cur);
      const tgt = pair(displayCurrency);
      if (!f || !tgt) return null;
      return convertMinor(minor, f, tgt);
    },
    [rates.data, displayCurrency],
  );

  return { displayCurrency, toDisplay, ratesMissing: rates.isError };
}

/**
 * Lazily asks the backend to fetch NBRB rates for currencies missing from the
 * local table (fresh install, new currency, offline first run), then reloads
 * the rates cache so conversions stop returning null.
 */
export function useEnsureRates(currencies: (string | undefined | null)[]) {
  const qc = useQueryClient();
  const rates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<RateRow>() });
  const have = new Set((rates.data ?? []).map((r) => r.currency));
  const missing = [
    ...new Set(currencies.filter((c): c is string => !!c && c !== "BYN" && !have.has(c))),
  ].sort();
  const key = missing.join(",");
  useEffect(() => {
    if (!key) return;
    let alive = true;
    (async () => {
      for (const c of key.split(",")) {
        try {
          await api.convertAt(100, c, "BYN", todayIso());
        } catch {
          // offline or NBRB has no rate — leave it missing.
        }
      }
      if (alive) qc.invalidateQueries({ queryKey: qk.rates });
    })();
    return () => {
      alive = false;
    };
  }, [key, qc]);
}
