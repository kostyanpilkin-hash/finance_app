import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import type { CategoryRow } from "@/lib/types";

export function CategoriesPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [tab, setTab] = useState<"expense" | "income">("expense");
  const cats = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories", true) });
  const [editing, setEditing] = useState<Partial<CategoryRow> | null>(null);
  const [moveTo, setMoveTo] = useState<Record<string, string>>({});

  const invalidate = () => qc.invalidateQueries({ queryKey: ["categories"] });
  const save = useMutation({
    mutationFn: (c: Record<string, unknown>) => api.categoryUpsert(c),
    onSuccess: () => { invalidate(); setEditing(null); },
  });
  const del = useMutation({
    mutationFn: ({ id, to }: { id: string; to?: string }) => api.categoryDelete(id, to),
    onSuccess: invalidate,
  });
  const restore = useMutation({
    mutationFn: (id: string) => api.entityRestore("categories", id),
    onSuccess: invalidate,
  });

  const list = (cats.data ?? []).filter((c) => c.kind === tab);
  const name = (c: CategoryRow) => (c.key ? t(`categories.${c.key}`, c.name) : c.name);

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.categories")}</h2>
        <Button onClick={() => setEditing({ kind: tab, color: "#6b7280", icon: "tag" })}>+ {t("ui.add")}</Button>
      </div>

      <div className="flex gap-2">
        {(["expense", "income"] as const).map((k) => (
          <Button key={k} size="sm" variant={tab === k ? "default" : "outline"} onClick={() => setTab(k)}>
            {t(`ui.${k}s`)}
          </Button>
        ))}
      </div>

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="flex flex-wrap items-center gap-3">
            <input className="flex-1 rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.name")}
              value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            <input className="w-28 rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.icon")}
              value={editing.icon ?? ""} onChange={(e) => setEditing({ ...editing, icon: e.target.value })} />
            <input type="color" value={editing.color ?? "#6b7280"} onChange={(e) => setEditing({ ...editing, color: e.target.value })} />
            <Button size="sm" disabled={!editing.name?.trim() || save.isPending} onClick={() => void save.mutateAsync(editing)}>
              {t("ui.save")}
            </Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
          </div>
        </div>
      )}

      <div className="space-y-2">
        {list.map((c) => (
          <div key={c.id} className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-sm">
            <span className="h-4 w-4 rounded-full" style={{ background: c.color }} />
            <span className="font-medium">{name(c)}</span>
            {c.isSystem === 1 && <span className="text-xs text-muted-foreground">({t("ui.system")})</span>}
            {c.archived === 1 && <span className="text-xs text-destructive">({t("ui.archived")})</span>}
            <div className="ml-auto flex items-center gap-1">
              {c.archived === 0 ? (
                <>
                  <Button size="sm" variant="ghost" onClick={() => setEditing(c)}>{t("ui.edit")}</Button>
                  {c.isSystem === 0 && (
                    <>
                      <select className="rounded border border-border bg-background px-2 py-1 text-xs"
                        value={moveTo[c.id] ?? ""} onChange={(e) => setMoveTo({ ...moveTo, [c.id]: e.target.value })}>
                        <option value="">{t("ui.moveToMisc")}</option>
                        {list.filter((x) => x.id !== c.id && x.archived === 0).map((x) => (
                          <option key={x.id} value={x.id}>{name(x)}</option>
                        ))}
                      </select>
                      <Button size="sm" variant="ghost" onClick={() => del.mutate({ id: c.id, to: moveTo[c.id] || undefined })}>
                        {t("ui.delete")}
                      </Button>
                    </>
                  )}
                </>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => restore.mutate(c.id)}>{t("ui.restore")}</Button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
