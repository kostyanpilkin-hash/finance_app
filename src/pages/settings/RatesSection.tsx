import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useLocalSettings } from "./useLocalSettings";

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

export function RatesSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const rates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<RateRow>() });
  const refresh = useMutation({
    mutationFn: api.ratesRefresh,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: qk.rates }),
  });
  return (
    <div className="space-y-2 text-sm">
      <Button size="sm" onClick={() => refresh.mutate()} loading={refresh.isPending}>
        {t("ui.refreshRates")}
      </Button>
      {(rates.data ?? []).map((r) => (
        <div key={r.currency} className="flex justify-between border-b border-border/50 pb-1">
          <span>{r.currency}</span>
          <span className="text-muted-foreground">
            {(r.rateBynScaled / 1e6).toFixed(4)} BYN · {r.date}
          </span>
        </div>
      ))}
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
