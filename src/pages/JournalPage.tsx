import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Bell, ChevronDown, GitMerge, Monitor, Pencil, Plus, RotateCcw, Server, Trash2, type LucideIcon } from "lucide-react";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useAccounts, useCategories, useUsers } from "@/lib/queries";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { dayLabel, formatMinor } from "@/lib/money";
import { cn } from "@/lib/utils";
import type { AuditRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { RefreshButton } from "@/components/RefreshButton";

const ENTITIES = ["transactions", "accounts", "categories", "goals", "reminders", "users"] as const;
const PAGE = 300;

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
interface AccountRef { id: string; name?: string; currency?: string }

const ACTION_META: Record<string, { icon: LucideIcon; cls: string }> = {
  create: { icon: Plus, cls: "text-income" },
  update: { icon: Pencil, cls: "text-primary" },
  delete: { icon: Trash2, cls: "text-expense" },
  restore: { icon: RotateCcw, cls: "text-primary" },
  conflict_resolved: { icon: GitMerge, cls: "text-muted-foreground" },
  merge_category: { icon: GitMerge, cls: "text-muted-foreground" },
};

const SOURCE_META: Record<string, { icon: LucideIcon; key: string }> = {
  bot: { icon: Bot, key: "journal.sourceBot" },
  reminder: { icon: Bell, key: "journal.sourceReminder" },
  system: { icon: Server, key: "journal.sourceSystem" },
};

function localDay(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function localTime(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso.slice(11, 16) : d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

function fullTime(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleString(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function JournalPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [entity, setEntity] = useState("");
  const [user, setUser] = useState("");
  const [origin, setOrigin] = useState("");
  const [showService, setShowService] = useState(false);
  const [pages, setPages] = useState(1);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const toggleRow = (id: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  const status = useQuery({ queryKey: qk.syncStatus, queryFn: api.syncStatus, staleTime: 60_000 });
  const log = useQuery({
    queryKey: qk.journal(search, entity, user, origin, showService, pages),
    queryFn: () =>
      api.journal<AuditRow>({
        search,
        entity: entity || undefined,
        userId: user || undefined,
        origin: origin || undefined,
        hideSystem: showService ? false : undefined,
        limit: PAGE * pages,
      }),
  });
  const accounts = useAccounts();
  const categories = useCategories();
  const users = useUsers();
  const reminders = useQuery({ queryKey: qk.entity("reminders"), queryFn: () => api.entityList<ReminderRef>("reminders") });
  const dayGroups = useMemo(() => {
    const m = new Map<string, AuditRow[]>();
    for (const e of log.data ?? []) {
      const d = localDay(e.ts);
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

  const acc = (id: unknown): AccountRef | undefined =>
    (accounts.data as AccountRef[] | undefined)?.find((a) => a.id === id);

  /** One-line human summary of what the entry is about. */
  const headline = (e: AuditRow, before: Record<string, unknown> | null, after: Record<string, unknown> | null): string | null => {
    const v = after ?? before;
    if (!v) return null;
    if (e.entity === "transactions") {
      const kind = String(v.kind ?? "expense");
      const cur = String(v.currency ?? "BYN");
      const comment = typeof v.comment === "string" && v.comment ? ` · ${v.comment}` : "";
      if (kind === "transfer" && v.toAccountId) {
        const from = acc(v.accountId);
        const to = acc(v.toAccountId);
        const toCur = to?.currency ?? cur;
        const toAmt = typeof v.toAmountMinor === "number" ? v.toAmountMinor : v.amountMinor;
        return (
          t("ui.journal.txTransfer", {
            amount: formatMinor(Number(v.amountMinor ?? 0), cur),
            account: from?.name ?? (v.accountId ? refName("accounts", String(v.accountId)) : "—"),
            toAmount: formatMinor(Number(toAmt ?? 0), toCur),
            toAccount: to?.name ?? refName("accounts", String(v.toAccountId)),
          }) + comment
        );
      }
      return (
        t("ui.journal.txSummary", {
          kind: t(`ui.txKind.${kind}`, kind),
          amount: formatMinor(Number(v.amountMinor ?? 0), cur),
          category: v.categoryId ? catLabel(String(v.categoryId)) : "—",
          account: acc(v.accountId)?.name ?? (v.accountId ? refName("accounts", String(v.accountId)) : "—"),
        }) + comment
      );
    }
    const name = v.name ?? v.title;
    if (name == null) return null;
    return t("ui.journal.entityAction", {
      entity: t(`ui.entity.${e.entity}`, e.entity),
      name: String(name),
      action: t(`ui.action.${e.action}`, e.action).toLowerCase(),
    });
  };

  /** fmt stays as before for the field-level diff rows. */
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

  /** Where the change came from: this device, a paired device, or a source. */
  const actor = (e: AuditRow): { icon: LucideIcon; label: string } => {
    if (e.isSelf === 1) return { icon: Monitor, label: t("ui.thisDevice") };
    const src = SOURCE_META[e.source];
    if (src) return { icon: src.icon, label: t(src.key) };
    if (e.dName) return { icon: Monitor, label: e.dName };
    return { icon: Server, label: t("ui.journal.host") };
  };

  // Origin filter options: this device + known sources. Remote devices are
  // reachable through the combined device/source match on the backend only
  // when their id is known — pairing gives the host every client's name/id.
  const originOptions = useMemo(() => {
    const opts: { value: string; label: string }[] = [{ value: "", label: t("ui.journal.allSources") }];
    const selfId = status.data?.deviceId;
    if (selfId) opts.push({ value: selfId, label: t("ui.thisDevice") });
    for (const [src, meta] of Object.entries(SOURCE_META)) {
      opts.push({ value: src, label: t(meta.key) });
    }
    return opts;
  }, [status.data?.deviceId, t]);

  const selectCls = "rounded-lg border border-border bg-background px-2 py-1.5 text-xs text-foreground";

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.journal")}</h2>
        <RefreshButton />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Input className="flex-1"
          placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />
        <select className={selectCls} value={user} onChange={(e) => setUser(e.target.value)} aria-label={t("ui.entered")}>
          <option value="">{t("ui.journal.allUsers")}</option>
          {(users.data ?? []).map((u) => (
            <option key={u.id} value={u.id}>{u.name}</option>
          ))}
        </select>
        <select className={selectCls} value={origin} onChange={(e) => setOrigin(e.target.value)} aria-label={t("ui.journal.allSources")}>
          {originOptions.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
        <Checkbox
          className="text-xs font-medium"
          checked={showService}
          label={t("ui.journal.showService")}
          onCheckedChange={(v) => setShowService(v)}
        />
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
                const A = actor(e);
                const title = headline(e, before, after)
                  ?? `${t(`ui.entity.${e.entity}`, e.entity)} · ${t(`ui.action.${e.action}`, e.action).toLowerCase()}`;
                const isOpen = open.has(e.id);
                return (
                  <div key={e.id}>
                    <button
                      type="button"
                      onClick={() => toggleRow(e.id)}
                      className="flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-sm transition-colors hover:bg-muted/40"
                    >
                      <span className={cn("grid h-6 w-6 shrink-0 place-items-center rounded-full bg-muted", meta.cls)}>
                        <AIcon className="h-3.5 w-3.5" />
                      </span>
                      <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{localTime(e.ts)}</span>
                      <span className="min-w-0 flex-1 truncate font-medium">{title}</span>
                      <span className="hidden shrink-0 items-center gap-1.5 text-xs text-muted-foreground sm:flex">
                        {e.uName && <>{e.uName} · </>}
                        <A.icon className="h-3 w-3" /> {A.label}
                      </span>
                      <ChevronDown className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform", isOpen && "rotate-180")} />
                    </button>
                    {isOpen && (
                      <div className="space-y-2.5 border-t border-border/50 bg-muted/20 px-4 py-3 text-sm sm:pl-[3.75rem]">
                        <div className="font-medium">{title}</div>
                        <div className="text-xs text-muted-foreground">
                          {t(`ui.entity.${e.entity}`, e.entity)} · {t(`ui.action.${e.action}`, e.action).toLowerCase()} · {fullTime(e.ts)}
                        </div>
                        {changes.length > 0 ? (
                          <div className="space-y-1">
                            {changes.map((c, i) => (
                              <div key={i} className="text-xs">
                                <span className="text-muted-foreground">{t(`ui.field.${c.field}`, c.field)}:</span>{" "}
                                <s className="text-muted-foreground/80">{fmt(c.field, c.before, after)}</s> →{" "}
                                <b>{fmt(c.field, c.after, after)}</b>
                              </div>
                            ))}
                          </div>
                        ) : e.action !== "delete" ? (
                          <div className="text-xs text-muted-foreground">{t("ui.noChanges")}</div>
                        ) : null}
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                          {e.uName && <span>{t("ui.entered")}: {e.uName}</span>}
                          <span className="inline-flex items-center gap-1"><A.icon className="h-3 w-3" /> {A.label}</span>
                        </div>
                        {e.action === "delete" && (
                          <Button size="sm" variant="outline"
                            onClick={() => restore.mutate({ entity: e.entity, id: e.entityId })}>
                            {t("ui.restore")}
                          </Button>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        ))}
        {log.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
        {(log.data?.length ?? 0) >= PAGE * pages && (
          <div className="flex justify-center pb-2">
            <Button size="sm" variant="outline" onClick={() => setPages((p) => p + 1)}>
              {t("ui.journal.showMore")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
