import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import type { AuditRow } from "@/lib/types";

export function JournalPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const log = useQuery({
    queryKey: ["journal", search],
    queryFn: () => api.journal<AuditRow>({ search, limit: 300 }),
  });
  const restore = useMutation({
    mutationFn: ({ entity, id }: { entity: string; id: string }) => api.entityRestore(entity as Parameters<typeof api.entityRestore>[0], id),
    onSuccess: () => qc.invalidateQueries(),
  });

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.journal")}</h2>
      <input className="w-full rounded border border-border bg-background px-3 py-2 text-sm"
        placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />
      <div className="space-y-1">
        {(log.data ?? []).map((e) => {
          let changes: { field: string; before: unknown; after: unknown }[] = [];
          try { changes = e.changesJson ? JSON.parse(e.changesJson) : []; } catch { /* ignore */ }
          const after = e.afterJson ? JSON.parse(e.afterJson) : null;
          return (
            <div key={e.id} className="rounded-xl border border-border bg-card px-3 py-2 text-sm">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>{e.ts.replace("T", " ").slice(0, 19)}</span>
                <span>· {t(`ui.entity.${e.entity}`, e.entity)}</span>
                <span>· {t(`ui.action.${e.action}`, e.action)}</span>
                {e.uName && <span>· {e.uName}</span>}
                {e.source === "bot" && <span>· 🤖</span>}
                {e.action === "delete" && (
                  <Button size="sm" variant="ghost" className="ml-auto"
                    onClick={() => restore.mutate({ entity: e.entity, id: e.entityId })}>
                    {t("ui.restore")}
                  </Button>
                )}
              </div>
              <div className="mt-1">
                {after?.name && <span className="font-medium">{String(after.name)} </span>}
                {changes.length === 0 && e.action !== "delete" && <span className="text-muted-foreground">{t("ui.noChanges")}</span>}
                {changes.slice(0, 6).map((c, i) => (
                  <div key={i} className="text-xs">
                    <span className="text-muted-foreground">{c.field}:</span>{" "}
                    <s>{c.before == null ? "—" : String(c.before)}</s> → <b>{c.after == null ? "—" : String(c.after)}</b>
                  </div>
                ))}
                {changes.length > 6 && <div className="text-xs text-muted-foreground">+{changes.length - 6}…</div>}
              </div>
            </div>
          );
        })}
        {log.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
