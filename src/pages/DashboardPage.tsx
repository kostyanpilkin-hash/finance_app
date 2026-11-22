import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { formatMinor, isoOf, todayIso } from "@/lib/money";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { qk } from "@/lib/queryKeys";
import { useCategories, useSettings, useUsers } from "@/lib/queries";
import { Pencil, PieChart } from "lucide-react";
import { DonutChart, type DonutItem } from "@/components/ui/donut-chart";
import { Empty } from "@/components/ui/empty";
import { CategoryTxDialog, type CategoryPeek } from "@/components/transactions/CategoryTxDialog";
import { AccountCard } from "@/components/accounts/AccountCard";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import type { DashboardData, RateOverview } from "@/lib/types";
import { RatesTable, type NbrbRow } from "@/components/RatesTable";
import { Segmented } from "@/components/ui/segmented";
import { RefreshButton } from "@/components/RefreshButton";
import { DashGrid } from "@/components/dashboard/DashGrid";
import { defaultLayout, normalizeLayout, type WidgetId } from "@/lib/dashLayout";
import { useMediaQuery } from "@/lib/useMediaQuery";
import { useLocalSettings } from "@/pages/settings/useLocalSettings";

type DashPeriod = "week" | "month" | "days30";
const PERIODS: DashPeriod[] = ["week", "month", "days30"];

function rangeFor(p: DashPeriod) {
  const now = new Date();
  const start = new Date(now);
  if (p === "week") start.setDate(now.getDate() - ((now.getDay() + 6) % 7));
  else if (p === "days30") start.setDate(now.getDate() - 29);
  else start.setDate(1);
  return { from: isoOf(start), to: todayIso() };
}

const FALLBACK_COLOR = "#9ca3af";

export function DashboardPage() {
  const { t } = useTranslation();
  const [period, setPeriod] = useState<DashPeriod>("month");
  const [peek, setPeek] = useState<CategoryPeek | null>(null);
  const [editing, setEditing] = useState(false);
  const isLg = useMediaQuery("(min-width: 1024px)");
  const { set } = useLocalSettings();
  const editMode = isLg && editing;
  const { from, to } = useMemo(() => rangeFor(period), [period]);
  const openPeek = useCallback(
    (kind: "expense" | "income") => (members: DonutItem[], label: string) =>
      setPeek({ title: label, kind, categoryIds: members.map((m) => m.id ?? "") }),
    [],
  );

  const { displayCurrency, toDisplay } = useDisplayMoney();
  const dash = useQuery({ queryKey: qk.dashboard(from, to), queryFn: () => api.dashboard<DashboardData>(from, to) });
  const sum = useQuery({
    queryKey: qk.analytics("summary", from, to, displayCurrency),
    queryFn: () => api.analyticsSummary(from, to, displayCurrency),
  });
  const categories = useCategories();
  const users = useUsers();
  const settings = useSettings();
  const chartOn = useCallback(
    (key: string) => (settings.data?.[key] ?? "1") === "1",
    [settings.data],
  );
  const ratesOn = chartOn("dash_rates");
  const overview = useQuery({ queryKey: ["ratesOverview"], queryFn: api.ratesOverview, staleTime: 60_000, enabled: ratesOn });
  const nbrbRates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<NbrbRow>(), enabled: ratesOn });

  const accounts = (dash.data?.accounts ?? []).filter((a) => !a.archived);
  const totals = dash.data?.totals ?? [];
  useEnsureRates([displayCurrency, ...accounts.map((a) => a.currency)]);

  const totalByn = useMemo(() => {
    let sum = 0;
    let missing = false;
    for (const a of accounts.filter((a) => a.includeInTotal !== 0)) {
      const v = toDisplay(a.balance ?? 0, a.currency);
      if (v == null) missing = true;
      else sum += v;
    }
    return { sum, currency: displayCurrency, missing };
  }, [accounts, toDisplay, displayCurrency]);

  const catSlices = useCallback((kind: "expense" | "income"): DonutItem[] => {
    const catOf = (id: string) => categories.data?.find((c) => c.id === id);
    const name = (id: string) => {
      const c = catOf(id);
      return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : t("ui.noCategory");
    };
    return (sum.data?.byCategory ?? [])
      .map((b) => ({ b, v: kind === "expense" ? b.expense : b.income }))
      .filter(({ v }) => v > 0)
      .map(({ b, v }) => {
        const id = b.key ?? "";
        return {
          id: id || null,
          name: name(id),
          value: v,
          fill: catOf(id)?.color ?? FALLBACK_COLOR,
          iconId: catOf(id)?.icon,
        };
      })
      .sort((a, b) => b.value - a.value);
  }, [sum.data, categories.data, t]);

  const peopleSlices = useCallback((kind: "expense" | "income"): DonutItem[] => {
    const userColor = new Map((users.data ?? []).map((u) => [u.id, u.color]));
    const userName = new Map((users.data ?? []).map((u) => [u.id, u.name]));
    return (sum.data?.byOwner ?? [])
      .map((b) => ({ b, v: kind === "expense" ? b.expense : b.income }))
      .filter(({ v }) => v > 0)
      .map(({ b, v }) => {
        const id = b.key;
        const name = (id ? userName.get(id) : undefined) ?? (id ? "?" : t("ui.noAuthor"));
        return {
          name,
          value: v,
          fill: id ? userColor.get(id) ?? FALLBACK_COLOR : FALLBACK_COLOR,
          initial: name[0]?.toUpperCase(),
        };
      })
      .sort((a, b) => b.value - a.value);
  }, [sum.data, users.data, t]);

  const card = "rounded-2xl border border-border bg-card p-4";
  const sumFor = (kind: string) => totals.filter((x) => x.kind === kind);
  const loading = dash.isLoading || sum.isLoading;

  const chartCards = [
    { id: "chart_pieMonth" as WidgetId, key: "pieMonth", on: true, data: catSlices("expense"), sel: openPeek("expense") },
    { id: "chart_incomeByCat" as WidgetId, key: "incomeByCat", on: chartOn("dash_chart_income_cat"), data: catSlices("income"), sel: openPeek("income") },
    { id: "chart_expenseByPeople" as WidgetId, key: "expenseByPeople", on: chartOn("dash_chart_expense_people"), data: peopleSlices("expense"), sel: undefined },
    { id: "chart_incomeByPeople" as WidgetId, key: "incomeByPeople", on: chartOn("dash_chart_income_people"), data: peopleSlices("income"), sel: undefined },
  ];

  const visible = useMemo<WidgetId[]>(() => {
    const v: WidgetId[] = ["stats"];
    for (const c of chartCards) if (c.on) v.push(c.id);
    if (ratesOn) v.push("rates");
    if (accounts.length > 0) v.push("accounts");
    return v;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.data, ratesOn, accounts.length]);

  const layout = useMemo(
    () => normalizeLayout(settings.data?.dash_layout, visible),
    [settings.data?.dash_layout, visible],
  );

  const renderWidget = (id: WidgetId) => {
    if (id === "stats") {
      return (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-3">
          <div className={card}>
            <div className="text-xs text-muted-foreground">{t("ui.totalBalance")}</div>
            <div className="font-display text-xl font-semibold">{formatMinor(totalByn.sum, totalByn.currency)}</div>
            {totalByn.missing && <div className="text-xs text-muted-foreground">{t("ui.ratesMissing")}</div>}
          </div>
          <div className={card}>
            <div className="text-xs text-muted-foreground">{t("ui.spentMonth")}</div>
            {sumFor("expense").map((x, i) => (
              <div key={i} className="text-lg font-semibold text-expense">−{formatMinor(x.amount, x.currency)}</div>
            ))}
            {sumFor("expense").length === 0 && <div className="text-lg font-semibold">0,00</div>}
          </div>
          <div className={card}>
            <div className="text-xs text-muted-foreground">{t("ui.incomeMonth")}</div>
            {sumFor("income").map((x, i) => (
              <div key={i} className="text-lg font-semibold text-income">+{formatMinor(x.amount, x.currency)}</div>
            ))}
            {sumFor("income").length === 0 && <div className="text-lg font-semibold">0,00</div>}
          </div>
        </div>
      );
    }
    if (id === "rates") {
      return <RatesCard t={t} overview={overview} nbrbRates={nbrbRates} card={card} />;
    }
    if (id === "accounts") {
      return (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
          {accounts.map((a) => (
            <AccountCard key={a.id} a={a} ownerName={a.ownerName ?? undefined} compact />
          ))}
        </div>
      );
    }
    const c = chartCards.find((x) => x.id === id);
    if (!c) return null;
    return (
      <div className={card}>
        <div className="mb-2 text-sm font-medium">{t(`ui.${c.key}`)}</div>
        {c.data.length > 0 ? (
          <DonutChart data={c.data} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")} onSelect={c.sel} />
        ) : (
          <Empty icon={PieChart} label={t("ui.emptyPeriod")} hint={t("ui.emptyDashHint")} />
        )}
      </div>
    );
  };

  const widgetTitle = (id: WidgetId): string => {
    switch (id) {
      case "stats": return t("ui.widgetStats");
      case "rates": return t("settings.dashRates");
      case "accounts": return t("ui.widgetAccounts");
      case "chart_pieMonth": return t("ui.pieMonth");
      case "chart_incomeByCat": return t("ui.incomeByCat");
      case "chart_expenseByPeople": return t("ui.expenseByPeople");
      case "chart_incomeByPeople": return t("ui.incomeByPeople");
    }
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.dashboard")}</h2>
        <div className="flex items-center gap-2">
          {editMode ? (
            <>
              <Button size="sm" variant="outline"
                onClick={() => set("dash_layout", JSON.stringify(defaultLayout(visible)))}>
                {t("ui.resetLayout")}
              </Button>
              <Button size="sm" onClick={() => setEditing(false)}>{t("ui.done")}</Button>
            </>
          ) : (
            isLg && (
              <Button size="icon-sm" variant="ghost" onClick={() => setEditing(true)}
                aria-label={t("ui.editLayout")} title={t("ui.editLayout")}>
                <Pencil className="h-4 w-4" />
              </Button>
            )
          )}
          <RefreshButton />
          <Segmented value={period} onChange={setPeriod}
            options={PERIODS.map((p) => ({ value: p, label: t(`ui.period.${p}`) }))} />
        </div>
      </div>
      {editMode && <p className="text-xs text-muted-foreground">{t("ui.editHint")}</p>}

      {loading ? (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Skeleton className="h-20" />
            <Skeleton className="h-20" />
            <Skeleton className="h-20" />
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <Skeleton className="h-64" />
            <Skeleton className="h-64" />
          </div>
        </>
      ) : isLg ? (
        <DashGrid
          layout={layout}
          editing={editMode}
          renderWidget={renderWidget}
          widgetTitle={widgetTitle}
          onChange={(l) => set("dash_layout", JSON.stringify(l))}
        />
      ) : (
        <>
          {renderWidget("stats")}
          {ratesOn && renderWidget("rates")}
          <div className="grid items-start gap-4 sm:grid-cols-2">
            {chartCards.filter((c) => c.on).map((c) => (
              <div key={c.id}>{renderWidget(c.id)}</div>
            ))}
          </div>
          {accounts.length > 0 && renderWidget("accounts")}
        </>
      )}
      <CategoryTxDialog peek={peek} from={from} to={to} onClose={() => setPeek(null)} />
    </div>
  );
}

function RatesCard({
  t,
  overview,
  nbrbRates,
  card,
}: {
  t: (k: string) => string;
  overview: { data?: RateOverview[]; isLoading: boolean; isSuccess: boolean };
  nbrbRates: { data?: NbrbRow[] };
  card: string;
}) {
  const rows: RateOverview[] = [];
  for (const cur of ["USD", "EUR", "RUB"]) {
    const o = overview.data?.find((r) => r.currency === cur);
    const n = nbrbRates.data?.find((r) => r.currency === cur);
    if (o || n) {
      rows.push({
        currency: cur,
        bsbApp: o?.bsbApp ?? null,
        bsbCard: o?.bsbCard ?? null,
        nbrb: o?.nbrb ?? (n ? Math.round(n.rateBynScaled / (n.scale || 1)) : null),
      });
    }
  }
  const nbrbDate = (nbrbRates.data ?? []).reduce((d, r) => (r.date > d ? r.date : d), "");
  return (
    <div className={`${card} min-w-0`}>
      <div className="mb-2 flex items-baseline justify-between gap-2">
        <div className="text-sm font-medium">{t("settings.dashRates")}</div>
        {nbrbDate && <div className="text-xs text-muted-foreground">{nbrbDate}</div>}
      </div>
      {overview.isLoading ? (
        <Skeleton className="h-16" />
      ) : rows.length === 0 ? (
        <div className="text-xs text-muted-foreground">{t("ui.ratesEmpty")}</div>
      ) : (
        <RatesTable rows={rows} />
      )}
    </div>
  );
}
