import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import { api } from "@/lib/api";
import { formatMinor, todayIso } from "@/lib/money";
import type { CategoryRow, DashboardData, TransactionRow } from "@/lib/types";

function monthRange() {
  const d = new Date();
  const from = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
  return { from, to: todayIso() };
}

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

export function DashboardPage() {
  const { t } = useTranslation();
  const { from, to } = monthRange();
  const dash = useQuery({ queryKey: ["dashboard", from, to], queryFn: () => api.dashboard<DashboardData>(from, to) });
  const tx = useQuery({
    queryKey: ["transactions", "month", from, to],
    queryFn: () => api.transactionList<TransactionRow>({ from, to, kind: "expense", limit: 2000 }),
  });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const rates = useQuery({ queryKey: ["rates"], queryFn: () => api.ratesLatest<RateRow>() });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const displayCurrency = settings.data?.display_currency || "BYN";

  const accounts = (dash.data?.accounts ?? []).filter((a) => !a.archived);
  const totals = dash.data?.totals ?? [];

  // All stored rates are scaled by 1e6; BYN is 1.0 = 1e6 scaled.
  const rateOf = useCallback(
    (cur: string) => (cur === "BYN" ? 1e6 : rates.data?.find((r) => r.currency === cur)?.rateBynScaled),
    [rates.data],
  );
  const toDisplay = useCallback(
    (minor: number, cur: string): number | null => {
      const r = rateOf(cur);
      const target = rateOf(displayCurrency);
      if (!r || !target) return null;
      return Math.round(minor * (r / 1e6) / (target / 1e6));
    },
    [rateOf, displayCurrency],
  );

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

  const pie = useMemo(() => {
    const curOf = new Map((dash.data?.accounts ?? []).map((a) => [a.id, a.currency]));
    const byCat = new Map<string, number>();
    for (const x of tx.data ?? []) {
      const v = toDisplay(x.amountMinor, curOf.get(x.accountId) ?? "BYN");
      if (v != null) byCat.set(x.categoryId ?? "", (byCat.get(x.categoryId ?? "") ?? 0) + v);
    }
    const name = (id: string) => {
      const c = categories.data?.find((c) => c.id === id);
      return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—";
    };
    const color = (id: string) => categories.data?.find((c) => c.id === id)?.color ?? "#9ca3af";
    return [...byCat.entries()]
      .map(([id, v]) => ({ name: name(id), value: v, fill: color(id) }))
      .sort((a, b) => b.value - a.value);
  }, [tx.data, categories.data, dash.data?.accounts, toDisplay, t]);

  const card = "rounded-2xl border border-border bg-card p-4";
  const sumFor = (kind: string) => totals.filter((x) => x.kind === kind);

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.dashboard")}</h2>

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

      {pie.length > 0 && (
        <div className={card}>
          <div className="mb-2 text-sm font-medium">{t("ui.pieMonth")}</div>
          <div className="h-64">
            <ResponsiveContainer>
              <PieChart>
                <Pie data={pie} dataKey="value" nameKey="name" innerRadius="45%" outerRadius="80%" paddingAngle={2}>
                  {pie.map((p, i) => <Cell key={i} fill={p.fill} />)}
                </Pie>
                <Tooltip formatter={(v) => formatMinor(Number(v), displayCurrency)} />
                <Legend />
              </PieChart>
            </ResponsiveContainer>
          </div>
        </div>
      )}

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {accounts.map((a) => (
          <div key={a.id} className={card}>
            <div className="mb-1 flex items-center gap-2 text-sm">
              <span className="h-3 w-3 rounded-full" style={{ background: a.color }} />
              <span className="font-medium">{a.name}</span>
              {a.last4 && <span className="text-xs text-muted-foreground">·{a.last4}</span>}
            </div>
            <div className="font-display text-xl font-semibold">{formatMinor(a.balance, a.currency)}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
