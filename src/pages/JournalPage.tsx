import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { GitMerge, Pencil, Plus, RotateCcw, Trash2, type LucideIcon } from "lucide-react";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useAccounts, useCategories, useUsers } from "@/lib/queries";
import { Button } from "@/components/ui/button";
import { dayLabel, formatMinor } from "@/lib/money";
import { cn } from "@/lib/utils";
import type { AuditRow } from "@/lib/types";
import { Input } from "@/components/ui/input";

const ENTITIES = ["transactions", "accounts", "categories", "goals", "reminders", "users"] as const;

const MONEY_FIELDS = new Set(["amountMinor", "toAmountMinor", "origAmountMinor", "targetMinor", "currentMinor", "balance"]);
const DATE_FIELDS = new Set(["occurredAt", "deadline", "nextDue", "dueDate", "paidAt", "createdAt"]);
const BOOL_FIELDS = new Set(["active", "archived", "includeInTotal", "isSystem"]);
const REF_FIELDS: Record<string, "accounts" | "categories" | "users" | "reminders"> = {
  accountId: "accounts",
  toAccountId: "accounts",
  categoryId: "categories",
  userId: "users",
  ownerId: "users",
  createdByUserId: "users",
  reminderId: "reminders",
};
const SKIP_FIELDS = new Set(["changesJson", "beforeJson", "afterJson"]);

interface ReminderRef { id: string; name: string }

const ACTION_META: Record<string, { icon: LucideIcon; cls: string }> = {
  create: { icon: Plus, cls: "text-income" },
  update: { icon: Pencil, cls: "text-primary" },
  delete: { icon: Trash2, cls: "text-expense" },
  restore: { icon: RotateCcw, cls: "text-primary" },
  conflict_resolved: { icon: GitMerge, cls: "text-muted-foreground" },
  merge_category: { icon: GitMerge, cls: "text-muted-foreground" },
};

export function JournalPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [entity, setEntity] = useState("");
  const log = useQuery({
    queryKey: qk.journal(search, entity),
    queryFn: () => api.journal<AuditRow>({ search, entity: entity || undefined, limit: 300 }),
  });
  const accounts = useAccounts();
  const categories = useCategories();
  const users = useUsers();
  const reminders = useQuery({ queryKey: qk.entity("reminders"), queryFn: () => api.entityList<ReminderRef>("reminders") });
  const dayGroups = useMemo(() => {
    const m = new Map<string, AuditRow[]>();
    for (const e of log.data ?? []) {
      const d = e.ts.slice(0, 10);
      const arr = m.get(d);
      if (arr) arr.push(e); else m.set(d, [e]);
    }
    return [...m.entries()].map(([day, items]) => ({ day, items }));
  }, [log.data]);

  const restore = useMutation({
    mutationFn: ({ entity, id }: { entity: string; id: string }) => api.entityRestore(entity as Parameters<typeof api.entityRestore>[0], id),
    onSuccess: () => qc.invalidateQueries(),
  });

  const refName = (table: string, id: string): string => {
    const list =
      table === "accounts" ? accounts.data
      : table === "categories" ? categories.data
      : table === "users" ? users.data
      : reminders.data;
    const row = list?.find((r) => r.id === id);
    if (!row) return id.slice(0, 8);
    if ("name" in row && row.name) return String(row.name);
    return id.slice(0, 8);
  };

  const catLabel = (id: string): string => {
    const c = categories.data?.find((x) => x.id === id);
    return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : id.slice(0, 8);
  };

  /** Human-readable value for a changed field. */
  const fmt = (field: string, v: unknown, after: Record<string, unknown> | null): ReactNode => {
    if (v == null) return "—";
    if (MONEY_FIELDS.has(field) && typeof v === "number") {
      return formatMinor(v, String(after?.currency ?? "BYN"));
    }
    if (DATE_FIELDS.has(field)) return String(v).replace("T", " ").slice(0, 16);
    if (BOOL_FIELDS.has(field)) return Number(v) ? t("ui.yes") : t("ui.no");
    if (field === "color") {
      return (
        <span className="inline-flex items-center gap-1 align-middle">
          <span className="inline-block h-3 w-3 rounded-full ring-1 ring-border" style={{ background: String(v) }} />
          {String(v)}
        </span>
      );
    }
    if (field === "categoryId") return catLabel(String(v));
    const ref = REF_FIELDS[field];
    if (ref) return refName(ref, String(v));
    return String(v);
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.journal")}</h2>
      <div className="flex flex-wrap items-center gap-2">
        <Input className="flex-1"
          placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />
      </div>
      <div className="flex flex-wrap gap-1.5">
        <button
          type="button"
          className={cn(
            "rounded-full border px-3 py-1 text-xs font-medium transition-colors",
            entity === "" ? "border-primary bg-primary/10 text-primary" : "border-border text-muted-foreground hover:text-foreground",
          )}
          onClick={() => setEntity("")}
        >
          {t("ui.all")}
        </button>
        {ENTITIES.map((e) => (
          <button
            key={e}
            type="button"
            className={cn(
              "rounded-full border px-3 py-1 text-xs font-medium capitalize transition-colors",
              entity === e ? "border-primary bg-primary/10 text-primary" : "border-border text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setEntity(e)}
          >
            {t(`ui.entity.${e}`, e)}
          </button>
        ))}
      </div>
      <div className="space-y-4">
        {dayGroups.map((g) => (
          <section key={g.day}>
            <h3 className="mb-1.5 px-1 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              {dayLabel(g.day, t)}
            </h3>
            <div className="divide-y divide-border/50 overflow-hidden rounded-2xl border border-border/80 bg-card/90">
              {g.items.map((e) => {
                let changes: { field: string; before: unknown; after: unknown }[] = [];
                try {
                  const parsed = e.changesJson ? JSON.parse(e.changesJson) : [];
                  changes = Array.isArray(parsed) ? parsed : [];
                } catch { /* ignore */ }
                changes = changes.filter((c) => !SKIP_FIELDS.has(c.field));
                let after: Record<string, unknown> | null = null;
                let before: Record<string, unknown> | null = null;
                try { after = e.afterJson ? JSON.parse(e.afterJson) : null; } catch { /* ignore */ }
                try { before = e.beforeJson ? JSON.parse(e.beforeJson) : null; } catch { /* ignore */ }
                const meta = ACTION_META[e.action] ?? { icon: Pencil, cls: "text-muted-foreground" };
                const AIcon = meta.icon;
                const rowName = after?.name ?? before?.name;
                return (
                  <div key={e.id} className="flex items-start gap-2.5 px-4 py-2.5 text-sm">
                    <span className={cn("mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-full bg-muted", meta.cls)}>
                      <AIcon className="h-3.5 w-3.5" />
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{e.ts.slice(11, 16)}</span>
                        <span className="text-xs text-muted-foreground">
                          {t(`ui.entity.${e.entity}`, e.entity)} · {t(`ui.action.${e.action}`, e.action)}
                          {e.uName ? ` · ${e.uName}` : ""}{e.source === "bot" ? " · 🤖" : ""}
                        </span>
                        {e.action === "delete" && (
                          <Button size="sm" variant="ghost" className="ml-auto -my-1"
                            onClick={() => restore.mutate({ entity: e.entity, id: e.entityId })}>
                            {t("ui.restore")}
                          </Button>
                        )}
                      </div>
                      {rowName != null && <div className="truncate font-medium">{String(rowName)}</div>}
                      {changes.length === 0 && e.action !== "delete" && rowName == null && (
                        <div className="text-xs text-muted-foreground">{t("ui.noChanges")}</div>
                      )}
                      {changes.slice(0, 6).map((c, i) => (
                        <div key={i} className="text-xs">
                          <span className="text-muted-foreground">{t(`ui.field.${c.field}`, c.field)}:</span>{" "}
                          <s className="text-muted-foreground/80">{fmt(c.field, c.before, after)}</s> →{" "}
                          <b>{fmt(c.field, c.after, after)}</b>
                        </div>
                      ))}
                      {changes.length > 6 && <div className="text-xs text-muted-foreground">+{changes.length - 6}…</div>}
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ))}
        {log.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
