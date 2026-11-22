import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Archive, Pencil } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useAccounts } from "@/lib/queries";
import { Button } from "@/components/ui/button";
import { TxForm } from "@/components/transactions/TxForm";
import { formatDateHuman, formatMinor, parseMinor } from "@/lib/money";
import { Input } from "@/components/ui/input";
import { RichSelect } from "@/components/ui/rich-select";
import { Field } from "@/components/ui/field";
import { ColorSwatches } from "@/components/ui/color-swatches";
import { RefreshButton } from "@/components/RefreshButton";

interface GoalRow {
  id: string; name: string; targetMinor: number; currentMinor: number;
  currency: string; deadline?: string | null; color: string; archived: number;
  accountId?: string | null;
}

export function GoalsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: qk.entity("goals", true), queryFn: () => api.entityList<GoalRow>("goals", true) });
  const accounts = useAccounts();
  const [editing, setEditing] = useState<Partial<GoalRow> | null>(null);
  const [depositing, setDepositing] = useState<GoalRow | null>(null);
  const [targetStr, setTargetStr] = useState("");
  const [currentStr, setCurrentStr] = useState("");

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: qk.entityAll("goals") });
    qc.invalidateQueries({ queryKey: qk.entityAll("accounts") });
  };
  const save = useMutation({
    mutationFn: (g: Record<string, unknown>) => api.goalUpsert(g),
    onSuccess: () => { invalidate(); setEditing(null); },
  });
  const del = useMutation({ mutationFn: (id: string) => api.entityDelete("goals", id), onSuccess: invalidate });
  const restore = useMutation({ mutationFn: (id: string) => api.entityRestore("goals", id), onSuccess: invalidate });

  const balanceOf = (id?: string | null) =>
    id ? accounts.data?.find((a) => a.id === id)?.balance ?? 0 : null;
  const currentOf = (g: GoalRow) => balanceOf(g.accountId) ?? g.currentMinor ?? 0;

  const open = (g?: GoalRow) => {
    setEditing(g ?? { currency: "BYN", color: "#059669", currentMinor: 0 });
    setTargetStr(g ? (g.targetMinor / 100).toString() : "");
    setCurrentStr(g ? (g.currentMinor / 100).toString() : "");
  };
  const submit = () => {
    if (!editing?.name?.trim()) return;
    void save.mutateAsync({ ...editing, targetMinor: parseMinor(targetStr) ?? 0, currentMinor: parseMinor(currentStr) ?? 0 });
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.goals")}</h2>
        <div className="flex items-center gap-2">
          <RefreshButton />
          <Button onClick={() => open()}>+ {t("ui.add")}</Button>
        </div>
      </div>

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("ui.name")}>
              <Input value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            </Field>
            <Field label={t("ui.deadline")}>
              <Input type="date"
                value={editing.deadline ?? ""} onChange={(e) => setEditing({ ...editing, deadline: e.target.value || null })} />
            </Field>
            <Field label={t("ui.goalTarget")}>
              <Input inputMode="decimal" value={targetStr} onChange={(e) => setTargetStr(e.target.value)} />
            </Field>
            {editing.accountId ? (
              <div className="flex items-center rounded border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
                {t("ui.goalCurrent")}: {formatMinor(balanceOf(editing.accountId) ?? 0, editing.currency)}
              </div>
            ) : (
              <Field label={t("ui.goalCurrent")}>
                <Input inputMode="decimal" value={currentStr} onChange={(e) => setCurrentStr(e.target.value)} />
              </Field>
            )}
            <Field label={t("ui.currency")}>
              <RichSelect
                value={editing.currency ?? "BYN"} onChange={(v) => setEditing({ ...editing, currency: v })}
                disabled={!!editing.accountId}
                items={["BYN", "USD", "EUR", "RUB"].map((c) => ({ value: c, label: c }))} />
            </Field>
            <Field label={t("ui.account")}>
              <RichSelect
                placeholder={t("ui.goalAccountNone")}
                value={editing.accountId ?? ""}
                onChange={(v) => {
                  const acc = (accounts.data ?? []).find((a) => a.id === v);
                  setEditing({
                    ...editing,
                    accountId: v || null,
                    currency: acc?.currency ?? editing.currency,
                  });
                }}
                items={[
                  { value: "", label: t("ui.goalAccountNone") },
                  ...(accounts.data ?? []).filter((a) => !a.archived).map((a) => ({
                    value: a.id, label: `${a.name} (${a.currency})`, dot: a.color,
                  })),
                ]} />
            </Field>
            <div>
              <div className="mb-2 text-xs font-medium text-muted-foreground">{t("ui.color")}</div>
              <ColorSwatches value={editing.color} onChange={(c) => setEditing({ ...editing, color: c })} />
            </div>
          </div>
          {editing.accountId && <p className="text-xs text-muted-foreground">{t("ui.goalAccountHint")}</p>}
          <div className="flex gap-2">
            <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
          </div>
        </div>
      )}

      {depositing && (
        <TxForm
          initial={{
            kind: "transfer",
            toAccountId: depositing.accountId ?? undefined,
            comment: depositing.name,
          }}
          onDone={() => {
            setDepositing(null);
            invalidate();
          }}
        />
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        {(list.data ?? []).map((g) => {
          const current = currentOf(g);
          const pct = g.targetMinor > 0 && Number.isFinite(current)
            ? Math.min(100, Math.round((current / g.targetMinor) * 100)) : 0;
          return (
            <div key={g.id} className="rounded-2xl border border-border bg-card p-4">
              <div className="mb-1 flex items-center gap-2">
                <span className="h-3 w-3 rounded-full" style={{ background: g.color }} />
                <span className="font-medium">{g.name}</span>
                {g.accountId && (
                  <span className="text-xs text-muted-foreground">
                    · {(accounts.data ?? []).find((a) => a.id === g.accountId)?.name}
                  </span>
                )}
                {g.deadline && <span className="ml-auto text-xs text-muted-foreground">{t("ui.till")} {formatDateHuman(g.deadline)}</span>}
              </div>
              <div className="mb-2 h-2 rounded-full bg-muted">
                <div className="h-2 rounded-full bg-primary" style={{ width: `${pct}%` }} />
              </div>
              <div className="flex items-center gap-2 text-sm">
                <span className="min-w-0 truncate">{formatMinor(current, g.currency)} / {formatMinor(g.targetMinor, g.currency)} · {pct}%</span>
                <div className="ml-auto flex shrink-0 items-center gap-0.5">
                  {g.archived === 0 ? (
                    <>
                      {g.accountId && (
                        <Button size="sm" variant="ghost" className="whitespace-nowrap" onClick={() => setDepositing(g)}>{t("ui.deposit")}</Button>
                      )}
                      <Button size="icon-sm" variant="ghost" title={t("ui.edit")} onClick={() => open(g)}>
                        <Pencil className="h-3.5 w-3.5" />
                      </Button>
                      <Button size="icon-sm" variant="ghost" title={t("ui.archive")}
                        className="text-muted-foreground hover:text-destructive"
                        onClick={() => del.mutate(g.id)}>
                        <Archive className="h-3.5 w-3.5" />
                      </Button>
                    </>
                  ) : (
                    <Button size="sm" variant="ghost" className="whitespace-nowrap" onClick={() => restore.mutate(g.id)}>{t("ui.restore")}</Button>
                  )}
                </div>
              </div>
            </div>
          );
        })}
        {list.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
