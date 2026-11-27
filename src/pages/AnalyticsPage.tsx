import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import {
  AreaChart, Area, Bar, ComposedChart, Line, XAxis, YAxis,
  Tooltip, ResponsiveContainer, CartesianGrid,
} from "recharts";
import { ChartColumn, PieChart, TrendingUp, Wallet } from "lucide-react";
import { api } from "@/lib/api";
import { formatMinor, formatPeriodHuman, todayIso } from "@/lib/money";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { qk } from "@/lib/queryKeys";
import { useAccounts, useCategories, useUsers } from "@/lib/queries";
import { slicePeek } from "@/lib/chartPeek";
import { DonutChart, type DonutItem } from "@/components/ui/donut-chart";
import { ChartHead, ChartLegend, ChartTooltip } from "@/components/ui/chart-kit";
import { AXIS, GRID, HOVER_CURSOR, compactTick } from "@/lib/chartStyle";
import { Empty } from "@/components/ui/empty";
import { CategoryTxDialog, type CategoryPeek } from "@/components/transactions/CategoryTxDialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import { RefreshButton } from "@/components/RefreshButton";

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

/** Local-midnight Date of a YYYY-MM-DD key. */
const dayOf = (key: string) => new Date(`${key}T00:00:00`);

const PALETTE = { expense: "var(--expense,#dc2626)", income: "var(--income,#16a34a)", net: "var(--primary)" };
const FALLBACK = "#9ca3af";

/** X tick of the by-day chart: weekends drawn fainter. */
function DayTick({ x, y, payload, fmt }: { x?: number; y?: number; payload?: { value: string }; fmt: (key: string) => string }) {
  const key = payload?.value ?? "";
  const wd = key ? dayOf(key).getDay() : 1;
  const weekend = wd === 0 || wd === 6;
  return (
    <text x={x} y={(y ?? 0) + 12} textAnchor="middle" fontSize={11} fill="var(--muted-foreground)" opacity={weekend ? 0.45 : 1}>
      {fmt(key)}
    </text>
  );
}

/** Percent change of `v` against `p`; null when there is nothing to compare with. */
const pctChange = (v: number, p: number | undefined) => (p ? Math.round(((v - p) / p) * 100) : null);

export function AnalyticsPage() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [kind, setKind] = useState<Kind>("month");
  const [anchor, setAnchor] = useState(() => new Date());
  const [custom, setCustom] = useState({ from: todayIso(), to: todayIso() });
  const [peek, setPeek] = useState<CategoryPeek | null>(null);
  const r = range(kind, anchor, custom);
  const pr = prevRange(kind, r);
  const m6from = monthsBack(6);
  const isCurrent = r.to >= todayIso() || kind === "custom";

  const { displayCurrency } = useDisplayMoney();
  const users = useUsers();
  const categories = useCategories();
  const accounts = useAccounts();
  useEnsureRates([displayCurrency, ...(accounts.data ?? []).map((a) => a.currency)]);

  const cur = useQuery({
    queryKey: qk.analytics("summary", r.from, r.to, displayCurrency),
    queryFn: () => api.analyticsSummary(r.from, r.to, displayCurrency),
  });
  const prev = useQuery({
    queryKey: qk.analytics("summary", pr.from, pr.to, displayCurrency),
    queryFn: () => api.analyticsSummary(pr.from, pr.to, displayCurrency),
  });
  const m6 = useQuery({
    queryKey: qk.analytics("monthly", m6from, displayCurrency),
    queryFn: () => api.analyticsMonthly(m6from, todayIso(), displayCurrency),
  });

  const e = cur.data?.expense ?? 0;
  const i = cur.data?.income ?? 0;
  const pe = prev.data?.expense ?? 0;
  const pi = prev.data?.income ?? 0;
  const delta = (a: number, b: number) => (b === 0 ? (a === 0 ? 0 : 100) : Math.round(((a - b) / b) * 100));

  const daysElapsed = Math.max(
    1,
    Math.round((new Date(Math.min(Date.parse(r.to), Date.parse(todayIso()))).getTime() - new Date(r.from).getTime()) / 86400000) + 1,
  );
  const daysInPeriod = Math.max(1, Math.round((new Date(r.to).getTime() - new Date(r.from).getTime()) / 86400000) + 1);
  const avgDaily = Math.round(e / daysElapsed);
  const forecast = kind === "month" && r.to >= todayIso() ? Math.round((e / daysElapsed) * daysInPeriod) : null;

  /** Amounts stay in minor units; the axis and tooltip format them. */
  const byDay = useMemo(
    () => (cur.data?.byDay ?? []).map((b) => ({ date: b.key ?? "", expense: b.expense, income: b.income })),
    [cur.data],
  );

  const balanceTrend = useMemo(() => {
    const out: { date: string; net: number }[] = [];
    let acc = 0;
    for (const b of byDay) {
      acc += b.income - b.expense;
      out.push({ date: b.date, net: acc });
    }
    return out;
  }, [byDay]);

  const byMonth = useMemo(
    () => (m6.data ?? []).map((b) => ({ month: b.key ?? "", expense: b.expense, income: b.income, net: b.income - b.expense })),
    [m6.data],
  );

  const tick = useMemo(() => compactTick(lang), [lang]);
  const dayTickFmt = (key: string) => {
    if (!key) return "";
    const d = dayOf(key);
    if (kind === "week") return d.toLocaleDateString(lang, { weekday: "short" });
    if (daysInPeriod > 40) return d.toLocaleDateString(lang, { day: "numeric", month: "numeric" });
    return String(d.getDate());
  };
  const dayLabel = (key: ReactNode) =>
    typeof key === "string" && key ? dayOf(key).toLocaleDateString(lang, { day: "numeric", month: "long", weekday: "short" }) : key;
  const monthOf = (key: string) => new Date(`${key}-01T00:00:00`);
  const monthTick = (key: string) => (key ? monthOf(key).toLocaleDateString(lang, { month: "short" }) : "");
  const monthLabel = (key: ReactNode) =>
    typeof key === "string" && key ? monthOf(key).toLocaleDateString(lang, { month: "long", year: "numeric" }) : key;

  const catDonut = (k: "expense" | "income"): DonutItem[] => {
    const catOf = (id: string) => categories.data?.find((c) => c.id === id);
    const name = (id: string) => {
      const c = catOf(id);
      return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : t("ui.noCategory");
    };
    const prevOf = new Map((prev.data?.byCategory ?? []).map((b) => [b.key ?? "", k === "expense" ? b.expense : b.income]));
    return (cur.data?.byCategory ?? [])
      .map((b) => ({ b, v: k === "expense" ? b.expense : b.income }))
      .filter(({ v }) => v > 0)
      .map(({ b, v }) => {
        const id = b.key ?? "";
        return {
          id: id || null,
          name: name(id),
          value: v,
          fill: catOf(id)?.color ?? FALLBACK,
          iconId: catOf(id)?.icon,
          delta: pctChange(v, prevOf.get(id)),
        };
      })
      .sort((a, b) => b.value - a.value);
  };
  const byCategory = { expense: catDonut("expense"), income: catDonut("income") };

  const byPeople = useMemo((): { expense: DonutItem[]; income: DonutItem[] } => {
    const colorOf = new Map((users.data ?? []).map((u) => [u.id, u.color]));
    const nameOf = new Map((users.data ?? []).map((u) => [u.id, u.name]));
    const mk = (k: "expense" | "income") =>
      (cur.data?.byOwner ?? [])
        .map((b) => {
          const id = b.key;
          const name = (id ? nameOf.get(id) : undefined) ?? (id ? "?" : t("ui.noAuthor"));
          return {
            id: id ?? null,
            name,
            value: k === "expense" ? b.expense : b.income,
            fill: id ? colorOf.get(id) ?? FALLBACK : FALLBACK,
            initial: name[0]?.toUpperCase(),
          };
        })
        .filter((d) => d.value > 0)
        .sort((a, b) => b.value - a.value);
    return { expense: mk("expense"), income: mk("income") };
  }, [cur.data, users.data, t]);

  const byAccount = useMemo(() => {
    const meta = new Map((accounts.data ?? []).map((a) => [a.id, a]));
    const rows = (cur.data?.byAccount ?? [])
      .filter((b) => b.expense > 0)
      .map((b) => {
        const id = b.key ?? "";
        const a = meta.get(id);
        return { id, name: a ? `${a.name}${a.last4 ? ` ·${a.last4}` : ""}` : id, value: b.expense, color: a?.color ?? FALLBACK };
      });
    const max = rows[0]?.value ?? 0;
    return rows.map((r) => ({ ...r, pct: max ? Math.round((r.value / max) * 100) : 0 }));
  }, [cur.data, accounts.data]);

  const openSlice = (k: "expense" | "income", by: "category" | "owner") => (members: DonutItem[], label: string) =>
    setPeek(slicePeek(k, by, members, label, displayCurrency));
  const openDay = (idx: unknown) => {
    const d = byDay[Number(idx)];
    if (!d) return;
    setPeek({
      title: String(dayLabel(d.date)),
      from: d.date,
      to: d.date,
      total: { expense: d.expense, income: d.income, currency: displayCurrency },
    });
  };
  const openMonth = (idx: unknown) => {
    const m = byMonth[Number(idx)];
    if (!m) return;
    setKind("month");
    setAnchor(monthOf(m.month));
  };
  const pickKind = (k: Kind) => {
    setKind(k);
    setAnchor(new Date());
  };

  const card = "rounded-2xl border border-border bg-card p-4";
  const stat = (label: string, v: number, pv: number, cls: string, suffix = "", signed = false) => {
    const d = delta(v, pv);
    return (
      <div className={card}>
        <div className="text-xs text-muted-foreground">{label}</div>
        <div className={cn("text-xl font-semibold", cls)}>{signed && v > 0 ? "+" : ""}{formatMinor(v)}{suffix ? ` ${suffix}` : ""}</div>
        <div className="text-xs text-muted-foreground">
          {t("ui.vsPrev")}: {pv ? formatMinor(pv) : "—"} {pv || v ? `(${d > 0 ? "+" : ""}${d}%)` : ""}
        </div>
      </div>
    );
  };
  const flowLegend = [
    { color: PALETTE.expense, label: t("ui.expenses") },
    { color: PALETTE.income, label: t("ui.incomes") },
  ];
  const donut = (data: DonutItem[], onSelect: (m: DonutItem[], l: string) => void, goodDelta: "up" | "down") =>
    data.length === 0 ? (
      <Empty icon={PieChart} label={t("ui.emptyPeriod")} hint={t("ui.emptyDashHint")} className="h-52" />
    ) : (
      <DonutChart data={data} currency={displayCurrency} totalLabel={t("ui.total")} otherLabel={t("ui.rest")}
        onSelect={onSelect} goodDelta={goodDelta} />
    );

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.analytics")}</h2>
        <RefreshButton />
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Segmented value={kind} onChange={pickKind}
          options={(["day", "week", "month", "custom"] as const).map((k) => ({ value: k, label: t(`ui.period.${k}`) }))} />
        {kind === "custom" && (
          <>
            <Input type="date" 
              value={custom.from} onChange={(e) => setCustom({ ...custom, from: e.target.value })} />
            <Input type="date" 
              value={custom.to} onChange={(e) => setCustom({ ...custom, to: e.target.value })} />
          </>
        )}
        <span className="text-sm text-muted-foreground">{formatPeriodHuman(r.from, r.to)}</span>
        {!isCurrent && (
          <Button size="sm" variant="ghost" onClick={() => setAnchor(new Date())}>{t("ui.backToCurrent")}</Button>
        )}
      </div>

      {cur.isLoading && (
        <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
          {Array.from({ length: 5 }).map((_, i) => <Skeleton key={i} className="h-20" />)}
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {stat(t("ui.expenses"), e, pe, "text-expense")}
        {stat(t("ui.incomes"), i, pi, "text-income")}
        {stat(t("ui.net"), i - e, pi - pe, i - e > 0 ? "text-income" : i - e < 0 ? "text-expense" : "", displayCurrency, true)}
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
        <ChartHead title={`${t("ui.byDay")} · ${displayCurrency}`}>
          {byDay.length > 0 && <ChartLegend items={flowLegend} />}
        </ChartHead>
        {byDay.length === 0 ? (
          <Empty icon={ChartColumn} label={t("ui.emptyPeriod")} hint={t("ui.emptyDashHint")} className="h-72" />
        ) : (
        <div className="h-72">
          <ResponsiveContainer>
            <ComposedChart data={byDay} onClick={(s) => openDay(s?.activeTooltipIndex)} className="cursor-pointer [&_*:focus:not(:focus-visible)]:outline-none">
              <CartesianGrid {...GRID} />
              <XAxis dataKey="date" {...AXIS} interval="preserveStartEnd" tick={<DayTick fmt={dayTickFmt} />} />
              <YAxis {...AXIS} width={48} tickFormatter={tick} />
              <Tooltip cursor={HOVER_CURSOR}
                content={<ChartTooltip currency={displayCurrency} formatLabel={dayLabel} hint={t("ui.clickDayHint")} />} />
              <Bar dataKey="expense" name={t("ui.expenses")} fill={PALETTE.expense} radius={[3, 3, 0, 0]} />
              <Bar dataKey="income" name={t("ui.incomes")} fill={PALETTE.income} radius={[3, 3, 0, 0]} />
            </ComposedChart>
          </ResponsiveContainer>
        </div>
        )}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <div className={card}>
          <ChartHead title={t("ui.byCategory")} />
          {donut(byCategory.expense, openSlice("expense", "category"), "down")}
        </div>
        <div className={card}>
          <ChartHead title={t("ui.incomeByCat")} />
          {donut(byCategory.income, openSlice("income", "category"), "up")}
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <div className={card}>
          <ChartHead title={`${t("ui.balanceTrend")} · ${displayCurrency}`} />
          {balanceTrend.length === 0 ? (
            <Empty icon={TrendingUp} label={t("ui.emptyPeriod")} hint={t("ui.emptyDashHint")} className="h-56" />
          ) : (
          <div className="h-56">
            <ResponsiveContainer>
              <AreaChart data={balanceTrend}>
                <defs>
                  <linearGradient id="netGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="var(--primary)" stopOpacity={0.35} />
                    <stop offset="100%" stopColor="var(--primary)" stopOpacity={0.02} />
                  </linearGradient>
                </defs>
                <CartesianGrid {...GRID} />
                <XAxis dataKey="date" {...AXIS} interval="preserveStartEnd" tick={<DayTick fmt={dayTickFmt} />} />
                <YAxis {...AXIS} width={48} tickFormatter={tick} />
                <Tooltip content={<ChartTooltip currency={displayCurrency} formatLabel={dayLabel} />} />
                <Area type="monotone" dataKey="net" name={t("ui.net")} stroke="var(--primary)" fill="url(#netGrad)" strokeWidth={2} />
              </AreaChart>
            </ResponsiveContainer>
          </div>
          )}
        </div>
        <div className={card}>
          <ChartHead title={`${t("ui.by6Months")} · ${displayCurrency}`}>
            <ChartLegend items={[...flowLegend, { color: PALETTE.net, label: t("ui.netShort"), line: true }]} />
          </ChartHead>
          <div className="h-56">
            <ResponsiveContainer>
              <ComposedChart data={byMonth} onClick={(s) => openMonth(s?.activeTooltipIndex)} className="cursor-pointer [&_*:focus:not(:focus-visible)]:outline-none">
                <CartesianGrid {...GRID} />
                <XAxis dataKey="month" {...AXIS} tickFormatter={monthTick} />
                <YAxis {...AXIS} width={48} tickFormatter={tick} />
                <Tooltip cursor={HOVER_CURSOR}
                  content={<ChartTooltip currency={displayCurrency} formatLabel={monthLabel} hint={t("ui.clickMonthHint")} />} />
                <Bar dataKey="expense" name={t("ui.expenses")} fill={PALETTE.expense} radius={[3, 3, 0, 0]} />
                <Bar dataKey="income" name={t("ui.incomes")} fill={PALETTE.income} radius={[3, 3, 0, 0]} />
                <Line type="monotone" dataKey="net" name={t("ui.netShort")} stroke={PALETTE.net} strokeWidth={2} dot={{ r: 3 }} />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className={card}>
          <ChartHead title={t("ui.byPeopleExpense")} />
          {donut(byPeople.expense, openSlice("expense", "owner"), "down")}
        </div>
        <div className={card}>
          <ChartHead title={t("ui.byPeopleIncome")} />
          {donut(byPeople.income, openSlice("income", "owner"), "up")}
        </div>
      </div>

      <div className={card}>
        <ChartHead title={`${t("ui.byAccounts")} · ${displayCurrency}`} />
        <div className="space-y-1">
          {byAccount.map((a) => (
            <button
              key={a.id}
              type="button"
              className="flex w-full cursor-pointer items-center gap-3 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted/60"
              onClick={() =>
                setPeek({ title: a.name, kind: "expense", accountId: a.id, total: { expense: a.value, currency: displayCurrency } })
              }
            >
              <span className="w-40 shrink-0 truncate text-muted-foreground">{a.name}</span>
              <div className="h-3 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full transition-[width] duration-500" style={{ width: `${a.pct}%`, background: a.color }} />
              </div>
              <span className="w-28 shrink-0 text-right font-medium">{formatMinor(a.value, displayCurrency)}</span>
            </button>
          ))}
          {byAccount.length === 0 && <Empty icon={Wallet} label={t("ui.emptyPeriod")} />}
        </div>
      </div>

      <CategoryTxDialog peek={peek} from={r.from} to={r.to} onClose={() => setPeek(null)} />
    </div>
  );
}
