import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import {
  AreaChart, Area, Bar, ComposedChart, Line, XAxis, YAxis,
  Tooltip, Legend, ResponsiveContainer, CartesianGrid,
} from "recharts";
import { api } from "@/lib/api";
import { formatMinor, todayIso } from "@/lib/money";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { DonutChart, type DonutItem } from "@/components/ui/donut-chart";
import { CategoryTxDialog, type CategoryPeek } from "@/components/transactions/CategoryTxDialog";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { AccountRow, CategoryRow, TransactionRow, UserRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";

type Kind = "day" | "week" | "month" | "custom";

function iso(d: Date) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function range(kind: Kind, anchor: Date, custom: { from: string; to: string }) {
  if (kind === "custom") return { from: custom.from, to: custom.to };
  const d = new Date(anchor);
  if (kind === "day") return { from: iso(d), to: iso(d) };
  if (kind === "week") {
    const wd = (d.getDay() + 6) % 7;
    const mon = new Date(d); mon.setDate(d.getDate() - wd);
    const sun = new Date(mon); sun.setDate(mon.getDate() + 6);
    return { from: iso(mon), to: iso(sun) };
  }
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const last = new Date(d.getFullYear(), d.getMonth() + 1, 0);
  return { from: iso(first), to: iso(last) };
}

function prevRange(kind: Kind, r: { from: string; to: string }) {
  const a = new Date(r.from), b = new Date(r.to);
  const shift = (d: Date, days: number) => { const x = new Date(d); x.setDate(x.getDate() - days); return x; };
  if (kind === "day") return { from: iso(shift(a, 1)), to: iso(shift(a, 1)) };
  if (kind === "week") return { from: iso(shift(a, 7)), to: iso(shift(b, 7)) };
  if (kind === "month") {
    const first = new Date(a.getFullYear(), a.getMonth() - 1, 1);
    const last = new Date(a.getFullYear(), a.getMonth(), 0);
    return { from: iso(first), to: iso(last) };
  }
  const len = Math.round((b.getTime() - a.getTime()) / 86400000) + 1;
  return { from: iso(shift(a, len)), to: iso(shift(b, len)) };
}

function monthsBack(n: number) {
  const d = new Date();
  return `${iso(new Date(d.getFullYear(), d.getMonth() - (n - 1), 1)).slice(0, 8)}01`;
}

const PALETTE = { expense: "var(--expense,#dc2626)", income: "var(--income,#16a34a)", net: "var(--primary)" };
const FALLBACK = "#9ca3af";

export function AnalyticsPage() {
  const { t, i18n } = useTranslation();
  const [kind, setKind] = useState<Kind>("month");
  const [custom, setCustom] = useState({ from: todayIso(), to: todayIso() });
  const [peek, setPeek] = useState<CategoryPeek | null>(null);
  const r = range(kind, new Date(), custom);
  const pr = prevRange(kind, r);
  const m6from = monthsBack(6);

  const cur = useQuery({
    queryKey: ["tx", "analytics", r.from, r.to],
    queryFn: () => api.transactionList<TransactionRow>({ from: r.from, to: r.to, limit: 5000 }),
  });
  const prev = useQuery({
    queryKey: ["tx", "analytics", pr.from, pr.to],
    queryFn: () => api.transactionList<TransactionRow>({ from: pr.from, to: pr.to, limit: 5000 }),
  });
  const m6 = useQuery({
    queryKey: ["tx", "analytics-m6", m6from],
    queryFn: () => api.transactionList<TransactionRow>({ from: m6from, to: todayIso(), limit: 20000 }),
  });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<UserRow>("users") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const { displayCurrency, toDisplay } = useDisplayMoney();
  useEnsureRates([displayCurrency, ...(accounts.data ?? []).map((a) => a.currency)]);

  /** tx -> converted minor in display currency (null when rate missing) */
  const conv = useMemo(() => {
    const fn = (rows: TransactionRow[] | undefined) => {
      const out: { x: TransactionRow; v: number }[] = [];
      for (const x of rows ?? []) {
        const v = toDisplay(x.amountMinor, x.aCurrency ?? "BYN");
        if (v != null) out.push({ x, v });
      }
      return out;
    };
    return {
      cur: fn(cur.data),
      prev: fn(prev.data),
      m6: fn(m6.data),
    };
  }, [cur.data, prev.data, m6.data, toDisplay]);

  const sum = (list: { x: TransactionRow; v: number }[], k: string) =>
    list.filter((e) => e.x.kind === k).reduce((s, e) => s + e.v, 0);

  const e = sum(conv.cur, "expense");
  const i = sum(conv.cur, "income");
  const pe = sum(conv.prev, "expense");
  const pi = sum(conv.prev, "income");
  const delta = (a: number, b: number) => (b === 0 ? (a === 0 ? 0 : 100) : Math.round(((a - b) / b) * 100));

  const daysElapsed = Math.max(
    1,
    Math.round((new Date(Math.min(Date.parse(r.to), Date.parse(todayIso()))).getTime() - new Date(r.from).getTime()) / 86400000) + 1,
  );
  const daysInPeriod = Math.max(1, Math.round((new Date(r.to).getTime() - new Date(r.from).getTime()) / 86400000) + 1);
  const avgDaily = Math.round(e / daysElapsed);
  const forecast = kind === "month" && r.to >= todayIso() ? Math.round((e / daysElapsed) * daysInPeriod) : null;

  const byDay = useMemo(() => {
    const buckets = new Map<string, { label: string; expense: number; income: number }>();
    for (const { x, v } of conv.cur) {
      if (x.kind === "transfer") continue;
      const day = x.occurredAt.slice(0, 10);
      const b = buckets.get(day) ?? { label: day.slice(5), expense: 0, income: 0 };
      b[x.kind as "expense" | "income"] += v;
      buckets.set(day, b);
    }
    return [...buckets.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, v]) => v);
  }, [conv.cur]);

  const balanceTrend = useMemo(() => {
    const out: { label: string; net: number }[] = [];
    let acc = 0;
    for (const b of byDay) {
      acc += b.income - b.expense;
      out.push({ label: b.label, net: acc / 100 });
    }
    return out;
  }, [byDay]);

  const byMonth = useMemo(() => {
    const buckets = new Map<string, { label: string; expense: number; income: number }>();
    for (const { x, v } of conv.m6) {
      if (x.kind === "transfer") continue;
      const key = x.occurredAt.slice(0, 7);
      const label = new Date(`${key}-01T00:00:00`).toLocaleDateString(i18n.language, { month: "short" });
      const b = buckets.get(key) ?? { label, expense: 0, income: 0 };
      b[x.kind as "expense" | "income"] += v;
      buckets.set(key, b);
    }
    return [...buckets.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([, v]) => ({ ...v, expense: v.expense / 100, income: v.income / 100, net: (v.income - v.expense) / 100 }));
  }, [conv.m6, i18n.language]);

  const byCategory = useMemo((): DonutItem[] => {
    const byCat = new Map<string, number>();
    for (const { x, v } of conv.cur) {
      if (x.kind !== "expense") continue;
      const id = x.categoryId ?? "";
      byCat.set(id, (byCat.get(id) ?? 0) + v);
    }
    const name = (id: string) => {
      const c = categories.data?.find((c) => c.id === id);
      return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : t("ui.noCategory");
    };
    const catOf = (id: string) => categories.data?.find((c) => c.id === id);
    return [...byCat.entries()]
      .map(([id, v]) => ({ id: id || null, name: name(id), value: v, fill: catOf(id)?.color ?? FALLBACK, iconId: catOf(id)?.icon }))
      .sort((a, b) => b.value - a.value);
  }, [conv.cur, categories.data, t]);

  const byPeople = useMemo((): { expense: DonutItem[]; income: DonutItem[] } => {
    const colorOf = new Map((users.data ?? []).map((u) => [u.id, u.color]));
    const nameOf = new Map((users.data ?? []).map((u) => [u.id, u.name]));
    const mk = (k: "expense" | "income") => {
      const byUser = new Map<string, { v: number; name: string }>();
      for (const { x, v } of conv.cur) {
        if (x.kind !== k) continue;
        // Entries are attributed to their owner (spent_by), author as fallback.
        const id = x.spentByUserId ?? x.createdByUserId ?? "";
        const who = x.suName ?? x.uName ?? nameOf.get(id) ?? "";
        const g = byUser.get(id) ?? { v: 0, name: who };
        g.v += v;
        byUser.set(id, g);
      }
      return [...byUser.entries()]
        .map(([id, g]) => {
          const name = g.name || (id ? "?" : t("ui.noAuthor"));
          return {
            name,
            value: g.v,
            fill: id ? colorOf.get(id) ?? FALLBACK : FALLBACK,
            initial: name[0]?.toUpperCase(),
          };
        })
        .sort((a, b) => b.value - a.value);
    };
    return { expense: mk("expense"), income: mk("income") };
  }, [conv.cur, users.data, t]);

  const byAccount = useMemo(() => {
    const byAcc = new Map<string, number>();
    for (const { x, v } of conv.cur) {
      if (x.kind !== "expense") continue;
      byAcc.set(x.accountId, (byAcc.get(x.accountId) ?? 0) + v);
    }
    const meta = new Map((accounts.data ?? []).map((a) => [a.id, a]));
    const rows = [...byAcc.entries()]
      .map(([id, v]) => {
        const a = meta.get(id);
        return { name: a ? `${a.name}${a.last4 ? ` ·${a.last4}` : ""}` : id, value: v, color: a?.color ?? FALLBACK };
      })
      .sort((a, b) => b.value - a.value);
    const max = rows[0]?.value ?? 0;
    return rows.map((r) => ({ ...r, pct: max ? Math.round((r.value / max) * 100) : 0 }));
  }, [conv.cur, accounts.data]);

  const card = "rounded-2xl border border-border bg-card p-4";
  const stat = (label: string, v: number, pv: number, cls: string, suffix = "") => {
    const d = delta(v, pv);
    return (
      <div className={card}>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className={cn("text-xl font-semibold", cls)}>{formatMinor(v)}{suffix ? ` ${suffix}` : ""}</div>
        <div className="text-xs text-muted-foreground">
          {t("ui.vsPrev")}: {pv ? formatMinor(pv) : "—"} {pv || v ? `(${d > 0 ? "+" : ""}${d}%)` : ""}
        </div>
      </div>
    );
  };

  const tooltipFmt = (v: unknown) => formatMinor(Math.round(Number(v) * 100), displayCurrency);

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.analytics")}</h2>

      <div className="flex flex-wrap items-center gap-2">
        <Segmented value={kind} onChange={setKind}
          options={(["day", "week", "month", "custom"] as const).map((k) => ({ value: k, label: t(`ui.period.${k}`) }))} />
        {kind === "custom" && (
          <>
            <Input type="date" 
              value={custom.from} onChange={(e) => setCustom({ ...custom, from: e.target.value })} />
            <Input type="date" 
              value={custom.to} onChange={(e) => setCustom({ ...custom, to: e.target.value })} />
          </>
        )}
        <span className="text-sm text-muted-foreground">{r.from} — {r.to}</span>
      </div>

      {cur.isLoading && (
        <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => <Skeleton key={i} className="h-20" />)}
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {stat(t("ui.expenses"), e, pe, "text-expense")}
        {stat(t("ui.incomes"), i, pi, "text-income")}
        {stat(t("ui.net"), i - e, pi - pe, "")}
        <div className={card}>
          <div className="text-xs text-muted-foreground">{t("ui.avgDaily")}</div>
          <div className="text-xl font-semibold">{formatMinor(avgDaily, displayCurrency)}</div>
        </div>
        <div className={card}>
          <div className="text-xs text-muted-foreground">{t("ui.forecast")}</div>
          <div className="text-xl font-semibold">{forecast !== null ? formatMinor(forecast, displayCurrency) : "—"}</div>
        </div>
      </div>

      <div className={card}>
        <div className="mb-2 text-sm font-medium">{t("ui.byDay")} · {displayCurrency}</div>
        <div className="h-72">
          <ResponsiveContainer>
            <ComposedChart data={byDay.map((b) => ({ ...b, expense: b.expense / 100, income: b.income / 100 }))}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
              <XAxis dataKey="label" fontSize={11} />
              <YAxis fontSize={11} />
              <Tooltip formatter={tooltipFmt} />
              <Legend />
              <Bar dataKey="expense" name={t("ui.expenses")} fill={PALETTE.expense} radius={[3, 3, 0, 0]} />
              <Bar dataKey="income" name={t("ui.incomes")} fill={PALETTE.income} radius={[3, 3, 0, 0]} />
            </ComposedChart>
          </ResponsiveContainer>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className={card}>
          <div className="mb-2 text-sm font-medium">{t("ui.balanceTrend")} · {displayCurrency}</div>
          <div className="h-56">
            <ResponsiveContainer>
              <AreaChart data={balanceTrend}>
                <defs>
                  <linearGradient id="netGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="var(--primary)" stopOpacity={0.35} />
                    <stop offset="100%" stopColor="var(--primary)" stopOpacity={0.02} />
                  </linearGradient>
                </defs>
                <XAxis dataKey="label" fontSize={11} />
                <YAxis fontSize={11} />
                <Tooltip formatter={tooltipFmt} />
                <Area type="monotone" dataKey="net" name={t("ui.net")} stroke="var(--primary)" fill="url(#netGrad)" strokeWidth={2} />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </div>
        <div className={card}>
          <div className="mb-2 text-sm font-medium">{t("ui.byCategory")}</div>
          <DonutChart data={byCategory} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")} emptyLabel={t("ui.empty")}
            onSelect={(members, label) => setPeek({ title: label, kind: "expense", categoryIds: members.map((m) => m.id ?? "") })} />
        </div>
      </div>

      <div className={card}>
        <div className="mb-2 text-sm font-medium">{t("ui.by6Months")} · {displayCurrency}</div>
        <div className="h-64">
          <ResponsiveContainer>
            <ComposedChart data={byMonth}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
              <XAxis dataKey="label" fontSize={11} />
              <YAxis fontSize={11} />
              <Tooltip formatter={tooltipFmt} />
              <Legend />
              <Bar dataKey="expense" name={t("ui.expenses")} fill={PALETTE.expense} radius={[3, 3, 0, 0]} />
              <Bar dataKey="income" name={t("ui.incomes")} fill={PALETTE.income} radius={[3, 3, 0, 0]} />
              <Line type="monotone" dataKey="net" name={t("ui.net")} stroke={PALETTE.net} strokeWidth={2} dot={{ r: 3 }} />
            </ComposedChart>
          </ResponsiveContainer>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className={card}>
          <div className="mb-2 text-sm font-medium">{t("ui.byPeopleExpense")}</div>
          <DonutChart data={byPeople.expense} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")} emptyLabel={t("ui.empty")} />
        </div>
        <div className={card}>
          <div className="mb-2 text-sm font-medium">{t("ui.byPeopleIncome")}</div>
          <DonutChart data={byPeople.income} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")} emptyLabel={t("ui.empty")} />
        </div>
      </div>

      <CategoryTxDialog peek={peek} from={r.from} to={r.to} onClose={() => setPeek(null)} />
      <div className={card}>
        <div className="mb-3 text-sm font-medium">{t("ui.byAccounts")} · {displayCurrency}</div>
        <div className="space-y-2">
          {byAccount.map((a) => (
            <div key={a.name} className="flex items-center gap-3 text-sm">
              <span className="w-40 shrink-0 truncate text-muted-foreground">{a.name}</span>
              <div className="h-3 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full transition-[width] duration-500" style={{ width: `${a.pct}%`, background: a.color }} />
              </div>
              <span className="w-28 shrink-0 text-right font-medium">{formatMinor(a.value, displayCurrency)}</span>
            </div>
          ))}
          {byAccount.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
        </div>
      </div>
    </div>
  );
}
