import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { formatMinor, todayIso } from "@/lib/money";
import type { DashboardData } from "@/lib/types";

function monthRange() {
  const d = new Date();
  const from = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
  return { from, to: todayIso() };
}

export function DashboardPage() {
  const { t } = useTranslation();
  const { from, to } = monthRange();
  const dash = useQuery({ queryKey: ["dashboard", from, to], queryFn: () => api.dashboard<DashboardData>(from, to) });
  const accounts = dash.data?.accounts ?? [];
  const totals = dash.data?.totals ?? [];
  const expense = totals.filter((x) => x.kind === "expense");
  const income = totals.filter((x) => x.kind === "income");

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.dashboard")}</h2>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="rounded-2xl border border-border bg-card p-4">
          <div className="text-xs text-muted-foreground">{t("ui.spentMonth")}</div>
          {expense.map((x, i) => (
            <div key={i} className="text-xl font-semibold text-expense">
              −{formatMinor(x["sum(t.amountMinor)"], x.currency)}
            </div>
          ))}
          {expense.length === 0 && <div className="text-xl font-semibold">0,00</div>}
        </div>
        <div className="rounded-2xl border border-border bg-card p-4">
          <div className="text-xs text-muted-foreground">{t("ui.incomeMonth")}</div>
          {income.map((x, i) => (
            <div key={i} className="text-xl font-semibold text-income">
              +{formatMinor(x["sum(t.amountMinor)"], x.currency)}
            </div>
          ))}
          {income.length === 0 && <div className="text-xl font-semibold">0,00</div>}
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {accounts.filter((a) => !a.archived).map((a) => (
          <div key={a.id} className="rounded-2xl border border-border bg-card p-4">
            <div className="mb-1 flex items-center gap-2 text-sm">
              <span className="h-3 w-3 rounded-full" style={{ background: a.color }} />
              <span className="font-medium">{a.name}</span>
              {a.last4 && <span className="text-xs text-muted-foreground">·{a.last4}</span>}
            </div>
            <div className="font-display text-xl font-semibold">{formatMinor(a.balance, a.currency)}</div>
          </div>
        ))}
      </div>
      {dash.isPending && <div className="text-sm text-muted-foreground">…</div>}
    </div>
  );
}
