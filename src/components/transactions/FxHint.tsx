import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { formatMinor } from "@/lib/money";
import { DEVIATION_WARN_PCT, RATE_KINDS, deviationPct, effectiveRate, formatRate, rateKindKey } from "@/lib/fx";
import type { FxSuggestReq, RateKind, RateSource } from "@/lib/types";
import { cn } from "@/lib/utils";

interface FxHintProps {
  /** What to convert; null hides the hint. */
  req: FxSuggestReq | null;
  /** Known amount's currency (entered / debited). */
  knownCurrency: string;
  /** Current value of the converted field (null = empty). */
  current: number | null;
  /** Source stored on the entry, shown as "Rate: …". */
  rateSource: RateSource | null | undefined;
  onFill: (amountMinor: number, kind: RateKind) => void;
}

/** Bank-rate suggestion for a converted amount: fill button (or reference
 *  text), rate switch, NBRB-fallback note and the actual-rate deviation. */
export function FxHint({ req, knownCurrency, current, rateSource, onFill }: FxHintProps) {
  const { t, i18n } = useTranslation();
  const [kind, setKind] = useState<RateKind | null>(null);
  const full = req ? { ...req, rateKind: kind } : null;
  const q = useQuery({
    queryKey: ["fx", full],
    queryFn: () => api.fxSuggest(full!),
    enabled: !!full && full.amountMinor > 0,
    staleTime: 60_000,
    placeholderData: (prev) => prev,
  });
  if (!req || !(req.amountMinor > 0)) return null;
  const s = q.data ?? null;
  const shown = kind ?? s?.defaultKind ?? "bsb_app";
  const label = s ? t(rateKindKey(s.rateKind)) : "";
  const rate = s?.rateScaled ? ` ${formatRate(s.rateScaled, foreignOf(knownCurrency, s.currency), i18n.language)}` : "";

  const actual = current && s ? effectiveRate(knownCurrency, req.amountMinor, s.currency, current) : null;
  const dev = actual && s?.rateScaled && current !== s.amountMinor ? deviationPct(actual, s.rateScaled) : null;

  return (
    <div className="flex w-full flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        {s && s.mode === "fill" && current !== s.amountMinor && !q.isPlaceholderData && (
          <Button type="button" size="sm" variant="outline" onClick={() => onFill(s.amountMinor, s.rateKind)}>
            ≈ {formatMinor(s.amountMinor, s.currency)} · {label}{rate}
          </Button>
        )}
        {s && s.mode === "reference" && (
          <span className="text-sm text-muted-foreground">
            {t("ui.fx.reference")}: ≈ {formatMinor(s.amountMinor, s.currency)} · {label}{rate}
          </span>
        )}
        <Segmented<RateKind> className="p-0.5" value={shown} onChange={setKind}
          options={RATE_KINDS.map((k) => ({ value: k, label: t(rateKindKey(k)) }))} />
      </div>
      {s?.fallback && <p className="text-xs text-muted-foreground">{t("ui.fx.fallback")}</p>}
      {dev !== null && actual !== null && s && (
        <p className={cn("text-xs", Math.abs(dev) >= DEVIATION_WARN_PCT ? "text-destructive" : "text-muted-foreground")}>
          {t("ui.fx.actual")} {formatRate(actual, foreignOf(knownCurrency, s.currency), i18n.language)} · {dev > 0 ? "+" : ""}{dev}% {t("ui.fx.vs")} «{label}»
        </p>
      )}
      {rateSource && !!current && (
        <p className="text-xs text-muted-foreground">{t("ui.fx.source")}: {t(rateKindKey(rateSource))}</p>
      )}
    </div>
  );
}

const foreignOf = (a: string, b: string) => (a.toUpperCase() === "BYN" ? b : a);
