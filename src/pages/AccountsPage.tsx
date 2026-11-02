import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import type { AccountRow, UserRow } from "@/lib/types";

const KINDS = ["card", "cash", "savings", "other"] as const;
const CURRENCIES = ["BYN", "USD", "EUR", "RUB"];

export function AccountsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts", true) });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<UserRow>("users") });
  const [editing, setEditing] = useState<Partial<AccountRow> | null>(null);

  const save = useMutation({
    mutationFn: (a: Record<string, unknown>) => api.accountUpsert(a),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["accounts"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setEditing(null);
    },
  });
  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("accounts", id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["accounts"] }),
  });
  const restore = useMutation({
    mutationFn: (id: string) => api.entityRestore("accounts", id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["accounts"] }),
  });

  const submit = () => {
    if (!editing?.name?.trim()) return;
    void save.mutateAsync(editing);
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.accounts")}</h2>
        <Button onClick={() => setEditing({ kind: "card", currency: "BYN", color: "#059669", includeInTotal: 1 })}>
          + {t("ui.add")}
        </Button>
      </div>

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.name")}
              value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            <select className="rounded border border-border bg-background px-3 py-2 text-sm" value={editing.kind ?? "card"}
              onChange={(e) => setEditing({ ...editing, kind: e.target.value as AccountRow["kind"] })}>
              {KINDS.map((k) => <option key={k} value={k}>{t(`ui.accountKind.${k}`)}</option>)}
            </select>
            {editing.kind === "card" && (
              <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.last4")}
                maxLength={4} value={editing.last4 ?? ""} onChange={(e) => setEditing({ ...editing, last4: e.target.value.replace(/\D/g, "") })} />
            )}
            <select className="rounded border border-border bg-background px-3 py-2 text-sm" value={editing.currency ?? "BYN"}
              onChange={(e) => setEditing({ ...editing, currency: e.target.value })}>
              {CURRENCIES.map((c) => <option key={c}>{c}</option>)}
            </select>
            <select className="rounded border border-border bg-background px-3 py-2 text-sm"
              value={editing.ownerUserId ?? ""} onChange={(e) => setEditing({ ...editing, ownerUserId: e.target.value || null })}>
              <option value="">{t("ui.noOwner")}</option>
              {users.data?.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
            </select>
            <label className="flex items-center gap-2 text-sm">
              {t("ui.color")}
              <input type="color" value={editing.color ?? "#059669"} onChange={(e) => setEditing({ ...editing, color: e.target.value })} />
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={(editing.includeInTotal ?? 1) === 1}
                onChange={(e) => setEditing({ ...editing, includeInTotal: e.target.checked ? 1 : 0 })} />
              {t("ui.includeInTotal")}
            </label>
          </div>
          <div className="flex gap-2">
            <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
            {save.isError && <span className="text-sm text-destructive">{String(save.error)}</span>}
          </div>
        </div>
      )}

      <div className="space-y-2">
        {(accounts.data ?? []).map((a) => (
          <div key={a.id} className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-sm">
            <span className="h-4 w-4 rounded-full" style={{ background: a.color }} />
            <span className="font-medium">{a.name}</span>
            {a.last4 && <span className="text-xs text-muted-foreground">·{a.last4}</span>}
            <span className="text-xs text-muted-foreground">{a.currency} · {t(`ui.accountKind.${a.kind}`)}</span>
            {a.archived === 1 && <span className="text-xs text-destructive">({t("ui.archived")})</span>}
            <div className="ml-auto flex gap-1">
              {a.archived === 0 ? (
                <>
                  <Button size="sm" variant="ghost" onClick={() => setEditing(a)}>{t("ui.edit")}</Button>
                  <Button size="sm" variant="ghost" onClick={() => del.mutate(a.id)}>{t("ui.archive")}</Button>
                </>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => restore.mutate(a.id)}>{t("ui.restore")}</Button>
              )}
            </div>
          </div>
        ))}
        {accounts.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
