import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { AccountCard, PsLogo } from "@/components/accounts/AccountCard";
import { useDisplayMoney, useEnsureRates } from "@/lib/useDisplayMoney";
import { Skeleton } from "@/components/ui/skeleton";
import { formatMinor } from "@/lib/money";
import { cn } from "@/lib/utils";
import type { AccountRow, DashboardData, PaymentSystem, UserRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Field } from "@/components/ui/field";
import { Checkbox } from "@/components/ui/checkbox";
import { ColorSwatches } from "@/components/ui/color-swatches";

const KINDS = ["card", "cash", "savings", "other"] as const;
const CURRENCIES = ["BYN", "USD", "EUR", "RUB"];
const PAYMENT_SYSTEMS: PaymentSystem[] = ["visa", "mastercard", "belkart", "mir", "unionpay"];
const PS_NAMES: Record<PaymentSystem, string> = {
  visa: "Visa", mastercard: "Mastercard", belkart: "Белкарт", mir: "Мир", unionpay: "UnionPay",
};

export function AccountsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const dash = useQuery({
    queryKey: ["dashboard", "accounts"],
    queryFn: () => api.dashboard<DashboardData>("1970-01-01", "2099-12-31"),
  });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<UserRow>("users") });
  const [editing, setEditing] = useState<Partial<AccountRow> | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const { displayCurrency, toDisplay } = useDisplayMoney();
  useEnsureRates([displayCurrency, ...(dash.data?.accounts ?? []).map((a) => a.currency)]);

  const save = useMutation({
    mutationFn: (a: Record<string, unknown>) => api.accountUpsert(a),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["accounts"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setEditing(null);
    },
  });
  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["accounts"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("accounts", id),
    onSuccess: invalidate,
  });
  const restore = useMutation({
    mutationFn: (id: string) => api.entityRestore("accounts", id),
    onSuccess: invalidate,
  });

  const submit = () => {
    if (!editing?.name?.trim()) return;
    void save.mutateAsync(editing);
  };

  const all = dash.data?.accounts ?? [];
  const active = all.filter((a) => a.archived === 0);
  const archived = all.filter((a) => a.archived === 1);
  const ownerOf = (a: AccountRow) => a.ownerName ?? undefined;

  const { totalDisplay, perCurrency } = useMemo(() => {
    let missing = false;
    let sum = 0;
    const per = new Map<string, number>();
    for (const a of active.filter((a) => a.includeInTotal !== 0)) {
      per.set(a.currency, (per.get(a.currency) ?? 0) + (a.balance ?? 0));
      const v = toDisplay(a.balance ?? 0, a.currency);
      if (v == null) missing = true;
      else sum += v;
    }
    return { totalDisplay: { sum, missing }, perCurrency: per };
  }, [active, toDisplay]);

  const preview: AccountRow = {
    id: editing?.id ?? "",
    name: editing?.name ?? "",
    kind: (editing?.kind ?? "card") as AccountRow["kind"],
    color: editing?.color ?? "#059669",
    currency: editing?.currency ?? "BYN",
    last4: editing?.last4 ?? null,
    bank: editing?.bank ?? null,
    paymentSystem: editing?.paymentSystem ?? null,
    includeInTotal: 1,
    archived: 0,
    sort: 0,
    balance: editing?.balance ?? 0,
    ownerName: undefined,
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.accounts")}</h2>
        <Button onClick={() => setEditing({ kind: "card", currency: "BYN", color: "#059669", includeInTotal: 1 })}>
          + {t("ui.add")}
        </Button>
      </div>

      <div className="rounded-2xl border border-border bg-card p-4">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <div>
            <div className="text-xs text-muted-foreground">{t("ui.totalOnAccounts")}</div>
            <div className="font-display text-xl font-semibold">{formatMinor(totalDisplay.sum, displayCurrency)}</div>
            {totalDisplay.missing && <div className="text-xs text-muted-foreground">{t("ui.ratesMissing")}</div>}
          </div>
          <div className="flex flex-wrap gap-2">
            {[...perCurrency.entries()].map(([cur, v]) => (
              <span key={cur} className="rounded-full bg-muted px-3 py-1 text-xs font-medium">
                {formatMinor(v, cur)}
              </span>
            ))}
          </div>
        </div>
      </div>

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="max-w-sm">
            <AccountCard a={preview} ownerName={users.data?.find((u) => u.id === editing.ownerUserId)?.name} />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("ui.name")}>
              <Input value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            </Field>
            <Field label={t("ui.kind")}>
              <Select value={editing.kind ?? "card"}
                onChange={(e) => setEditing({ ...editing, kind: e.target.value as AccountRow["kind"] })}>
                {KINDS.map((k) => <option key={k} value={k}>{t(`ui.accountKind.${k}`)}</option>)}
              </Select>
            </Field>
            {editing.kind === "card" && (
              <>
                <Field label={t("ui.bank")}>
                  <Input value={editing.bank ?? ""} onChange={(e) => setEditing({ ...editing, bank: e.target.value })} />
                </Field>
                <Field label={t("ui.last4")}>
                  <Input maxLength={4} value={editing.last4 ?? ""} onChange={(e) => setEditing({ ...editing, last4: e.target.value.replace(/\D/g, "") })} />
                </Field>
                <div className="flex flex-wrap items-center gap-1.5 sm:col-span-2">
                  <span className="text-xs text-muted-foreground">{t("ui.paymentSystem")}:</span>
                  <button type="button"
                    className={cn("rounded-lg border px-2 py-1 text-xs transition-colors", !editing.paymentSystem ? "border-primary bg-primary/10" : "border-border bg-background hover:bg-muted")}
                    onClick={() => setEditing({ ...editing, paymentSystem: null })}>
                    —
                  </button>
                  {PAYMENT_SYSTEMS.map((ps) => (
                    <button key={ps} type="button"
                      className={cn("flex items-center gap-1 rounded-lg border px-2 py-1 text-xs transition-colors",
                        editing.paymentSystem === ps ? "border-primary bg-primary/10" : "border-border bg-background hover:bg-muted")}
                      onClick={() => setEditing({ ...editing, paymentSystem: ps })}>
                      <span className="text-foreground"><PsLogo ps={ps} /></span>
                      {PS_NAMES[ps]}
                    </button>
                  ))}
                </div>
              </>
            )}
            <Field label={t("ui.currency")}>
              <Select value={editing.currency ?? "BYN"}
                onChange={(e) => setEditing({ ...editing, currency: e.target.value })}>
                {CURRENCIES.map((c) => <option key={c}>{c}</option>)}
              </Select>
            </Field>
            <Field label={t("ui.owner")}>
              <Select
                value={editing.ownerUserId ?? ""} onChange={(e) => setEditing({ ...editing, ownerUserId: e.target.value || null })}>
                <option value="">{t("ui.noOwner")}</option>
                {users.data?.map((u) => <option key={u.id} value={u.id}>{u.name}</option>)}
              </Select>
            </Field>
            <div>
              <div className="mb-2 text-xs font-medium text-muted-foreground">{t("ui.color")}</div>
              <ColorSwatches value={editing.color} onChange={(c) => setEditing({ ...editing, color: c })} />
            </div>
            <Checkbox checked={(editing.includeInTotal ?? 1) === 1} label={t("ui.includeInTotal")}
              onCheckedChange={(v) => setEditing({ ...editing, includeInTotal: v ? 1 : 0 })} />
          </div>
          <div className="flex gap-2">
            <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
            {save.isError && <span className="text-sm text-destructive">{String(save.error)}</span>}
          </div>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        {dash.isLoading &&
          Array.from({ length: 4 }).map((_, i) => <Skeleton key={i} className="h-44 rounded-2xl" />)}
        {active.map((a) => (
          <AccountCard key={a.id} a={a} ownerName={ownerOf(a)}
            onEdit={() => setEditing(a)}
            onArchive={() => del.mutate(a.id)} />
        ))}
        {active.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>

      {archived.length > 0 && (
        <div>
          <button type="button" className="mb-3 flex items-center gap-2 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
            onClick={() => setShowArchived((v) => !v)}>
            <span className={cn("transition-transform", showArchived && "rotate-90")}>▸</span>
            {t("ui.archivedAccounts")} ({archived.length})
          </button>
          {showArchived && (
            <div className="grid gap-4 sm:grid-cols-2">
              {archived.map((a) => (
                <div key={a.id} className="relative">
                  <AccountCard a={a} ownerName={ownerOf(a)} />
                  <Button size="sm" variant="outline" className="absolute right-2 top-2 h-7 text-xs"
                    onClick={() => restore.mutate(a.id)}>
                    {t("ui.restore")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
