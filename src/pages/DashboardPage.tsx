import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { formatMinor, isoOf, todayIso } from "@/lib/money";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { DonutChart, type DonutItem } from "@/components/ui/donut-chart";
import { AccountCard } from "@/components/accounts/AccountCard";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { CategoryRow, DashboardData, TransactionRow, UserRow } from "@/lib/types";

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
  const { from, to } = useMemo(() => rangeFor(period), [period]);

  const dash = useQuery({ queryKey: ["dashboard", from, to], queryFn: () => api.dashboard<DashboardData>(from, to) });
  const tx = useQuery({
    queryKey: ["transactions", "dash", from, to],
    queryFn: () => api.transactionList<TransactionRow>({ from, to, limit: 2000 }),
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<UserRow>("users") });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const { displayCurrency, toDisplay } = useDisplayMoney();
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

  // Transactions converted to the display currency once; charts group the result.
  const converted = useMemo(() => {
    const curOf = new Map((dash.data?.accounts ?? []).map((a) => [a.id, a.currency]));
    const out: { x: TransactionRow; v: number }[] = [];
    for (const x of tx.data ?? []) {
      const v = toDisplay(x.amountMinor, curOf.get(x.accountId) ?? "BYN");
      if (v != null) out.push({ x, v });
    }
    return out;
  }, [tx.data, dash.data?.accounts, toDisplay]);

  const catSlices = useCallback((kind: "expense" | "income"): DonutItem[] => {
    const byCat = new Map<string, number>();
    for (const { x, v } of converted) {
      if (x.kind !== kind) continue;
      const id = x.categoryId ?? "";
      byCat.set(id, (byCat.get(id) ?? 0) + v);
    }
    const catOf = (id: string) => categories.data?.find((c) => c.id === id);
    const name = (id: string) => {
      const c = catOf(id);
      return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : t("ui.noCategory");
    };
    return [...byCat.entries()]
      .map(([id, v]) => ({
        name: name(id),
        value: v,
        fill: catOf(id)?.color ?? FALLBACK_COLOR,
        iconId: catOf(id)?.icon,
      }))
      .sort((a, b) => b.value - a.value);
  }, [converted, categories.data, t]);

  const peopleSlices = useCallback((kind: "expense" | "income"): DonutItem[] => {
    const userColor = new Map((users.data ?? []).map((u) => [u.id, u.color]));
    const userName = new Map((users.data ?? []).map((u) => [u.id, u.name]));
    const byUser = new Map<string, { value: number; name: string }>();
    for (const { x, v } of converted) {
      if (x.kind !== kind) continue;
      const id = x.createdByUserId ?? "";
      const g = byUser.get(id) ?? { value: 0, name: id ? (x.uName ?? userName.get(id) ?? "") : "" };
      g.value += v;
      byUser.set(id, g);
    }
    return [...byUser.entries()]
      .map(([id, g]) => {
        const name = g.name || (id ? "?" : t("ui.noAuthor"));
        return {
          name,
          value: g.value,
          fill: id ? userColor.get(id) ?? FALLBACK_COLOR : FALLBACK_COLOR,
          initial: name[0]?.toUpperCase(),
        };
      })
      .sort((a, b) => b.value - a.value);
  }, [converted, users.data, t]);

  const card = "rounded-2xl border border-border bg-card p-4";
  const sumFor = (kind: string) => totals.filter((x) => x.kind === kind);
  const loading = dash.isLoading || tx.isLoading;

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.dashboard")}</h2>
        <div className="flex gap-1 rounded-xl border border-border bg-card p-1">
          {PERIODS.map((p) => (
            <button
              key={p}
              type="button"
              className={cn(
                "rounded-lg px-3 py-1 text-xs font-medium transition-colors",
                period === p ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setPeriod(p)}
            >
              {t(`ui.period.${p}`)}
            </button>
          ))}
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

          <div className="grid gap-4 sm:grid-cols-2">
            <div className={card}>
              <div className="mb-2 text-sm font-medium">{t("ui.pieMonth")}</div>
              <DonutChart data={catSlices("expense")} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.other")} />
            </div>
            {chartOn("dash_chart_income_cat") && (
              <div className={card}>
                <div className="mb-2 text-sm font-medium">{t("ui.incomeByCat")}</div>
                <DonutChart data={catSlices("income")} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.other")} />
              </div>
            )}
            {chartOn("dash_chart_expense_people") && (
              <div className={card}>
                <div className="mb-2 text-sm font-medium">{t("ui.expenseByPeople")}</div>
                <DonutChart data={peopleSlices("expense")} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.other")} />
              </div>
            )}
            {chartOn("dash_chart_income_people") && (
              <div className={card}>
                <div className="mb-2 text-sm font-medium">{t("ui.incomeByPeople")}</div>
                <DonutChart data={peopleSlices("income")} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.other")} />
              </div>
            )}
          </div>

          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {accounts.map((a) => (
              <AccountCard key={a.id} a={a} ownerName={a.ownerName ?? undefined} compact />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
