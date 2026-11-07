import { useCallback, useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { todayIso } from "@/lib/money";

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

/**
 * Display currency + conversion of any minor-unit amount into it.
 * Returns null from toDisplay when a rate is missing.
 */
export function useDisplayMoney() {
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const rates = useQuery({ queryKey: ["rates"], queryFn: () => api.ratesLatest<RateRow>() });
  const displayCurrency = settings.data?.display_currency || "BYN";

  // All stored rates are scaled by 1e6; BYN is 1.0 = 1e6 scaled.
  const toDisplay = useCallback(
    (minor: number, cur: string): number | null => {
      const rate = (c: string) => (c === "BYN" ? 1e6 : rates.data?.find((r) => r.currency === c)?.rateBynScaled);
      const r = rate(cur);
      const target = rate(displayCurrency);
      if (!r || !target) return null;
      return Math.round(minor * (r / 1e6) / (target / 1e6));
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
  const rates = useQuery({ queryKey: ["rates"], queryFn: () => api.ratesLatest<RateRow>() });
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
      if (alive) qc.invalidateQueries({ queryKey: ["rates"] });
    })();
    return () => {
      alive = false;
    };
  }, [key, qc]);
}
