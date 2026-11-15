import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Trash2 } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk, invalidateMoney } from "@/lib/queryKeys";
import { useAccounts, useCategories } from "@/lib/queries";
import { Button } from "@/components/ui/button";
import { TxForm } from "@/components/transactions/TxForm";
import { formatDateHuman, formatMinor, parseMinor, nowLocalIso } from "@/lib/money";
import { Input } from "@/components/ui/input";
import { RichSelect } from "@/components/ui/rich-select";
import { CategoryIcon } from "@/components/ui/category-icon";
import { Field } from "@/components/ui/field";

interface ReminderRow {
  id: string; name: string; amountMinor?: number | null; currency: string;
  kind: string; ruleType: string; ruleDay: number; ruleMonth?: number | null;
  nextDue?: string | null; active: number; accountId?: string | null; categoryId?: string | null;
}

export function RemindersPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: qk.entity("reminders", true), queryFn: () => api.entityList<ReminderRow>("reminders", true) });
  const due = useQuery({ queryKey: qk.remindersDue, queryFn: () => api.remindersDue<ReminderRow>(), refetchInterval: 60000 });
  const accounts = useAccounts();
  const categories = useCategories();
  const [editing, setEditing] = useState<Partial<ReminderRow> | null>(null);
  const [paying, setPaying] = useState<ReminderRow | null>(null);
  const [amountStr, setAmountStr] = useState("");

  const invalidate = () => { qc.invalidateQueries({ queryKey: qk.entityAll("reminders") }); qc.invalidateQueries({ queryKey: qk.remindersDue }); };
  const save = useMutation({
    mutationFn: (r: Record<string, unknown>) => api.reminderUpsert(r),
    onSuccess: () => { invalidate(); setEditing(null); setAmountStr(""); },
  });
  const resolve = useMutation({
    mutationFn: ({ id, status, tx }: { id: string; status: string; tx?: Record<string, unknown> }) =>
      api.reminderResolve(id, status, tx),
    onSuccess: () => { invalidate(); invalidateMoney(qc); },
  });
  const del = useMutation({ mutationFn: (id: string) => api.entityDelete("reminders", id), onSuccess: invalidate });

  const ruleLabel = (r: ReminderRow) => {
    if (r.ruleType === "weekly") return `${t("ui.rule.weekly")} · ${t(`ui.wd.${r.ruleDay}`)}`;
    if (r.ruleType === "yearly") return `${t("ui.rule.yearly")} · ${r.ruleDay}.${String(r.ruleMonth ?? 1).padStart(2, "0")}`;
    return `${t("ui.rule.monthly")} · ${r.ruleDay ?? "—"}`;
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.reminders")}</h2>
        <Button onClick={() => setEditing({ kind: "expense", ruleType: "monthly", ruleDay: 1, currency: "BYN", active: 1 })}>
          + {t("ui.add")}
        </Button>
      </div>

      {(due.data ?? []).length > 0 && (
        <div className="rounded-2xl border border-destructive/40 bg-card p-4">
          <div className="mb-2 font-medium text-destructive">{t("ui.dueToday")}</div>
          {due.data!.map((r) => (
            <div key={r.id} className="mb-1 flex items-center gap-3 text-sm">
              <span className="font-medium">{r.name}</span>
              {r.amountMinor != null && <span>{formatMinor(r.amountMinor, r.currency)}</span>}
              <div className="ml-auto flex gap-1">
                <Button size="sm" onClick={() => setPaying(r)}>{t("ui.record")}</Button>
                <Button size="sm" variant="outline" onClick={() => resolve.mutate({ id: r.id, status: "skipped" })}>{t("ui.skip")}</Button>
              </div>
            </div>
          ))}
        </div>
      )}

      {paying && (
        <TxForm
          initial={{
            kind: paying.kind === "income" ? "income" : "expense",
            amountMinor: paying.amountMinor ?? 0,
            accountId: paying.accountId ?? undefined,
            categoryId: paying.categoryId ?? undefined,
            comment: paying.name,
            occurredAt: nowLocalIso(),
            source: "reminder",
          }}
          onDone={(saved) => {
            const id = paying.id;
            setPaying(null);
            if (saved) resolve.mutate({ id, status: "done" });
          }}
        />
      )}

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("ui.name")}>
              <Input value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            </Field>
            <Field label={t("ui.amount")}>
              <Input inputMode="decimal" value={amountStr}
              onChange={(e) => { setAmountStr(e.target.value); setEditing({ ...editing, amountMinor: parseMinor(e.target.value) ?? null }); }} />
            </Field>
            <Field label={t("ui.frequency")}>
              <RichSelect
                value={editing.ruleType ?? "monthly"} onChange={(v) => setEditing({ ...editing, ruleType: v })}
                items={[
                  { value: "monthly", label: t("ui.rule.monthly") },
                  { value: "weekly", label: t("ui.rule.weekly") },
                  { value: "yearly", label: t("ui.rule.yearly") },
                ]} />
            </Field>
            <Field label={editing.ruleType === "weekly" ? t("ui.wdDay") : t("ui.day")}>
              <Input type="number" min={1} max={editing.ruleType === "weekly" ? 7 : 31}
                value={editing.ruleDay ?? 1} onChange={(e) => setEditing({ ...editing, ruleDay: Number(e.target.value) })} />
            </Field>
            {editing.ruleType === "yearly" && (
              <Field label={t("ui.period.month")}>
                <Input type="number" min={1} max={12}
                  value={editing.ruleMonth ?? 1}
                  onChange={(e) => setEditing({ ...editing, ruleMonth: Number(e.target.value) })} />
              </Field>
            )}
            <Field label={t("ui.kind")}>
              <RichSelect
                value={editing.kind ?? "expense"} onChange={(v) => setEditing({ ...editing, kind: v })}
                items={[
                  { value: "expense", label: t("ui.txKind.expense") },
                  { value: "income", label: t("ui.txKind.income") },
                ]} />
            </Field>
            <Field label={t("ui.account")}>
              <RichSelect
                placeholder={t("ui.account")}
                value={editing.accountId ?? ""} onChange={(v) => setEditing({ ...editing, accountId: v || null })}
                items={[
                  { value: "", label: t("ui.account") },
                  ...(accounts.data ?? []).map((a) => ({ value: a.id, label: a.name, dot: a.color })),
                ]} />
            </Field>
            <Field label={t("ui.category")}>
              <RichSelect
                placeholder={t("ui.category")}
                value={editing.categoryId ?? ""} onChange={(v) => setEditing({ ...editing, categoryId: v || null })}
                items={[
                  { value: "", label: t("ui.category") },
                  ...(categories.data ?? []).filter((c) => c.kind === (editing.kind ?? "expense")).map((c) => ({
                    value: c.id, label: c.key ? t(`categories.${c.key}`, c.name) : c.name,
                    icon: <CategoryIcon icon={c.icon} color={c.color} size={20} />,
                  })),
                ]} />
            </Field>
          </div>
          <div className="flex gap-2">
            <Button size="sm" disabled={!editing.name?.trim() || save.isPending} onClick={() => void save.mutateAsync(editing)}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
          </div>
        </div>
      )}

      <div className="space-y-2">
        {(list.data ?? []).map((r) => (
          <div key={r.id} className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-sm">
            <span className="font-medium">{r.name}</span>
            {r.amountMinor != null && <span className={r.kind === "expense" ? "text-expense" : "text-income"}>{formatMinor(r.amountMinor, r.currency)}</span>}
            <span className="text-xs text-muted-foreground">{ruleLabel(r)}</span>
            <span className="ml-auto text-xs text-muted-foreground">{t("ui.nextDue")}: {r.nextDue ? formatDateHuman(r.nextDue) : "—"}</span>
            <Button size="icon-sm" variant="ghost" title={t("ui.edit")}
              onClick={() => { setEditing(r); setAmountStr(r.amountMinor ? (r.amountMinor / 100).toString() : ""); }}>
              <Pencil className="h-3.5 w-3.5" />
            </Button>
            <Button size="icon-sm" variant="ghost" title={t("ui.delete")}
              className="text-muted-foreground hover:text-destructive"
              onClick={() => del.mutate(r.id)}>
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        ))}
        {list.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
