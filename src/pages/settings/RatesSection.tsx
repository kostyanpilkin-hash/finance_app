import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useLocalSettings } from "./useLocalSettings";
import { formatRate, rateUnits } from "@/lib/fx";
import type { RateOverview } from "@/lib/types";

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

export function RatesSection() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const rates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<RateRow>() });
  const overview = useQuery({ queryKey: ["ratesOverview"], queryFn: api.ratesOverview, staleTime: 60_000 });
  const refresh = useMutation({
    mutationFn: api.ratesRefresh,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: qk.rates });
      void queryClient.invalidateQueries({ queryKey: ["ratesOverview"] });
    },
  });
  const rows: RateOverview[] = [...(overview.data ?? [])];
  for (const r of rates.data ?? []) {
    if (!rows.some((x) => x.currency === r.currency)) {
      rows.push({ currency: r.currency, bsbApp: null, bsbCard: null, nbrb: Math.round(r.rateBynScaled / (r.scale || 1)) });
    }
  }
  const nbrbDate = (rates.data ?? []).reduce((d, r) => (r.date > d ? r.date : d), "");
  const bsbMissing = overview.isSuccess && rows.slice(0, 4).every((r) => !r.bsbApp && !r.bsbCard);
  const fmt = (v: number | null | undefined, cur: string) => (v ? formatRate(v, cur, i18n.language) : "—");
  const pair = (p: RateOverview["bsbApp"], cur: string) => (p ? `${fmt(p.buy, cur)} / ${fmt(p.sell, cur)}` : "—");
  return (
    <div className="space-y-2 text-sm">
      <Button size="sm" onClick={() => refresh.mutate()} loading={refresh.isPending || overview.isFetching}>
        {t("ui.refreshRates")}
      </Button>
      <div className="overflow-x-auto">
        <table className="w-full text-left tabular-nums">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-1 pr-3 font-medium" />
              <th className="py-1 pr-3 font-medium">{t("ui.fx.bsbApp")} <span className="font-normal">({t("ui.fx.buy")} / {t("ui.fx.sell")})</span></th>
              <th className="py-1 pr-3 font-medium">{t("ui.fx.bsbCard")}</th>
              <th className="py-1 font-medium">{t("ui.fx.nbrb")}{nbrbDate && <span className="font-normal"> · {nbrbDate}</span>}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.currency} className="border-t border-border/50">
                <td className="py-1 pr-3 font-medium">
                  {rateUnits(r.currency) > 1 ? `${rateUnits(r.currency)} ` : ""}{r.currency}
                </td>
                <td className="py-1 pr-3">{pair(r.bsbApp, r.currency)}</td>
                <td className="py-1 pr-3">{pair(r.bsbCard, r.currency)}</td>
                <td className="py-1">{fmt(r.nbrb, r.currency)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {bsbMissing && <p className="text-xs text-muted-foreground">{t("ui.fx.fallback")}</p>}
      {refresh.data && (
        <p className="text-xs text-muted-foreground">
          {t("ui.refreshed")}: {refresh.data.updated.join(", ")}
          {refresh.data.skipped.length > 0 && ` · ${t("ui.skipped")}: ${refresh.data.skipped.join(", ")}`}
        </p>
      )}
    </div>
  );
}

const DISPLAY_CURRENCIES = ["BYN", "USD", "EUR"];

export function DisplayCurrency() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  const current = get("display_currency", "BYN");
  return (
    <div className="mt-4">
      <div className="mb-2 text-sm font-medium">{t("settings.displayCurrency")}</div>
      <div className="flex flex-wrap gap-2">
        {DISPLAY_CURRENCIES.map((c) => (
          <Button key={c} size="sm" variant={current === c ? "default" : "outline"} onClick={() => set("display_currency", c)}>{c}</Button>
        ))}
      </div>
      <p className="mt-1 text-xs text-muted-foreground">{t("settings.displayCurrencyHint")}</p>
    </div>
  );
}

const DASH_CHARTS: { key: string; label: string }[] = [
  { key: "dash_chart_income_cat", label: "settings.dashChartIncomeCat" },
  { key: "dash_chart_expense_people", label: "settings.dashChartExpensePeople" },
  { key: "dash_chart_income_people", label: "settings.dashChartIncomePeople" },
];

export function DashboardCharts() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  return (
    <div className="mt-4 border-t border-border/50 pt-4">
      <div className="mb-2 text-sm font-medium">{t("settings.dashCharts")}</div>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {DASH_CHARTS.map(({ key, label }) => (
          <Checkbox key={key} className="font-medium" checked={get(key, "1") === "1"} label={t(label)}
            onCheckedChange={(v) => set(key, v ? "1" : "0")} />
        ))}
      </div>
    </div>
  );
}
