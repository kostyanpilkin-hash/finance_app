import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { formatMinor, isoOf, todayIso } from "@/lib/money";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { qk } from "@/lib/queryKeys";
import { useCategories, useSettings, useUsers } from "@/lib/queries";
import { PieChart } from "lucide-react";
import { DonutChart, type DonutItem } from "@/components/ui/donut-chart";
import { Empty } from "@/components/ui/empty";
import { CategoryTxDialog, type CategoryPeek } from "@/components/transactions/CategoryTxDialog";
import { AccountCard } from "@/components/accounts/AccountCard";
import { Skeleton } from "@/components/ui/skeleton";
import type { DashboardData } from "@/lib/types";
import { Segmented } from "@/components/ui/segmented";
import { RefreshButton } from "@/components/RefreshButton";

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

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.dashboard")}</h2>
        <div className="flex items-center gap-2">
          <RefreshButton />
          <Segmented value={period} onChange={setPeriod}
            options={PERIODS.map((p) => ({ value: p, label: t(`ui.period.${p}`) }))} />
        </div>
      </div>

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
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
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

          <div className="grid gap-4 lg:grid-cols-2">
            {(
              [
                { key: "pieMonth", on: true, data: catSlices("expense"), sel: openPeek("expense") },
                { key: "incomeByCat", on: chartOn("dash_chart_income_cat"), data: catSlices("income"), sel: openPeek("income") },
                { key: "expenseByPeople", on: chartOn("dash_chart_expense_people"), data: peopleSlices("expense"), sel: undefined },
                { key: "incomeByPeople", on: chartOn("dash_chart_income_people"), data: peopleSlices("income"), sel: undefined },
              ] as const
            ).filter((c) => c.on).map((c) => (
              <div key={c.key} className={card}>
                <div className="mb-2 text-sm font-medium">{t(`ui.${c.key}`)}</div>
                {c.data.length > 0 ? (
                  <DonutChart data={c.data} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")} onSelect={c.sel} />
                ) : (
                  <Empty icon={PieChart} label={t("ui.emptyPeriod")} hint={t("ui.emptyDashHint")} />
                )}
              </div>
            ))}
          </div>

          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {accounts.map((a) => (
              <AccountCard key={a.id} a={a} ownerName={a.ownerName ?? undefined} compact />
            ))}
          </div>
        </>
      )}
      <CategoryTxDialog peek={peek} from={from} to={to} onClose={() => setPeek(null)} />
    </div>
  );
}
