import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { TxForm } from "@/components/transactions/TxForm";
import { formatMinor, parseMinor, nowLocalIso } from "@/lib/money";
import type { AccountRow, CategoryRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Field } from "@/components/ui/field";

interface ReminderRow {
  id: string; name: string; amountMinor?: number | null; currency: string;
  kind: string; ruleType: string; ruleDay: number; ruleMonth?: number | null;
  nextDue?: string | null; active: number; accountId?: string | null; categoryId?: string | null;
}

export function RemindersPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["reminders"], queryFn: () => api.entityList<ReminderRow>("reminders", true) });
  const due = useQuery({ queryKey: ["reminders-due"], queryFn: () => api.remindersDue<ReminderRow>(), refetchInterval: 60000 });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const [editing, setEditing] = useState<Partial<ReminderRow> | null>(null);
  const [paying, setPaying] = useState<ReminderRow | null>(null);
  const [amountStr, setAmountStr] = useState("");

  const invalidate = () => { qc.invalidateQueries({ queryKey: ["reminders"] }); qc.invalidateQueries({ queryKey: ["reminders-due"] }); };
  const save = useMutation({
    mutationFn: (r: Record<string, unknown>) => api.reminderUpsert(r),
    onSuccess: () => { invalidate(); setEditing(null); setAmountStr(""); },
  });
  const resolve = useMutation({
    mutationFn: ({ id, status, tx }: { id: string; status: string; tx?: Record<string, unknown> }) =>
      api.reminderResolve(id, status, tx),
    onSuccess: invalidate,
  });
  const del = useMutation({ mutationFn: (id: string) => api.entityDelete("reminders", id), onSuccess: invalidate });

  const ruleLabel = (r: ReminderRow) => {
    if (r.ruleType === "weekly") return `${t("ui.rule.weekly")} · ${t(`ui.wd.${r.ruleDay}`)}`;
    if (r.ruleType === "yearly") return `${t("ui.rule.yearly")} · ${r.ruleDay}.${String(r.ruleMonth ?? 1).padStart(2, "0")}`;
    return `${t("ui.rule.monthly")} · ${r.ruleDay}`;
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
              <Select
                value={editing.ruleType ?? "monthly"} onChange={(e) => setEditing({ ...editing, ruleType: e.target.value })}>
                <option value="monthly">{t("ui.rule.monthly")}</option>
                <option value="weekly">{t("ui.rule.weekly")}</option>
                <option value="yearly">{t("ui.rule.yearly")}</option>
              </Select>
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
              <Select
                value={editing.kind ?? "expense"} onChange={(e) => setEditing({ ...editing, kind: e.target.value })}>
                <option value="expense">{t("ui.txKind.expense")}</option>
                <option value="income">{t("ui.txKind.income")}</option>
              </Select>
            </Field>
            <Field label={t("ui.account")}>
              <Select
                value={editing.accountId ?? ""} onChange={(e) => setEditing({ ...editing, accountId: e.target.value || null })}>
                <option value="">{t("ui.account")}</option>
                {(accounts.data ?? []).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
              </Select>
            </Field>
            <Field label={t("ui.category")}>
              <Select
                value={editing.categoryId ?? ""} onChange={(e) => setEditing({ ...editing, categoryId: e.target.value || null })}>
                <option value="">{t("ui.category")}</option>
                {(categories.data ?? []).filter((c) => c.kind === (editing.kind ?? "expense")).map((c) => (
                  <option key={c.id} value={c.id}>{c.key ? t(`categories.${c.key}`, c.name) : c.name}</option>
                ))}
              </Select>
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
            <span className="ml-auto text-xs text-muted-foreground">{t("ui.nextDue")}: {r.nextDue ?? "—"}</span>
            <Button size="sm" variant="ghost" onClick={() => { setEditing(r); setAmountStr(r.amountMinor ? (r.amountMinor / 100).toString() : ""); }}>{t("ui.edit")}</Button>
            <Button size="sm" variant="ghost" onClick={() => del.mutate(r.id)}>{t("ui.delete")}</Button>
          </div>
        ))}
        {list.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
