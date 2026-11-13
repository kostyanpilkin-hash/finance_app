import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useCategories } from "@/lib/queries";
import { Button } from "@/components/ui/button";
import { CategoryIcon } from "@/components/ui/category-icon";
import { CATEGORY_ICONS, NO_ICON } from "@/lib/categoryIcons";
import { cn } from "@/lib/utils";
import type { CategoryRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";
import { Select } from "@/components/ui/select";
import { ColorSwatches } from "@/components/ui/color-swatches";

export function CategoriesPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [tab, setTab] = useState<"expense" | "income">("expense");
  const cats = useCategories(true);
  const [editing, setEditing] = useState<Partial<CategoryRow> | null>(null);
  const [moveTo, setMoveTo] = useState<Record<string, string>>({});

  const invalidate = () => qc.invalidateQueries({ queryKey: qk.entityAll("categories") });
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
        <Button onClick={() => setEditing({ kind: tab, color: "#6b7280", icon: NO_ICON })}>+ {t("ui.add")}</Button>
      </div>

      <div className="flex gap-2">
        {(["expense", "income"] as const).map((k) => (
          <Button key={k} size="sm" variant={tab === k ? "default" : "outline"} onClick={() => setTab(k)}>
            {t(`ui.${k}s`)}
          </Button>
        ))}
      </div>

      {editing && (
        <div className="space-y-4 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="flex flex-wrap items-end gap-3">
            <CategoryIcon icon={editing.icon} color={editing.color} size={44} />
            <Field label={t("ui.name")} className="min-w-48 flex-1">
              <Input value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            </Field>
            <Button size="sm" disabled={!editing.name?.trim() || save.isPending} onClick={() => void save.mutateAsync(editing)}>
              {t("ui.save")}
            </Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
          </div>

          <div>
            <div className="mb-2 text-xs font-medium text-muted-foreground">{t("ui.icon")}</div>
            <div className="grid grid-cols-8 gap-2 sm:grid-cols-12">
              {[{ id: NO_ICON }, ...CATEGORY_ICONS].map(({ id }) => (
                <button key={id} type="button" title={id === NO_ICON ? t("ui.noIcon") : id}
                  onClick={() => setEditing({ ...editing, icon: id })}
                  className={cn("flex items-center justify-center rounded-full p-0.5 transition hover:scale-110",
                    (editing.icon ?? NO_ICON) === id ? "ring-2 ring-primary ring-offset-2 ring-offset-card" : "")}>
                  {id === NO_ICON ? (
                    <span className="flex h-9 w-9 items-center justify-center rounded-full border-2 border-dashed border-border text-xs text-muted-foreground">∅</span>
                  ) : (
                    <CategoryIcon icon={id} color={editing.color} size={36} />
                  )}
                </button>
              ))}
            </div>
          </div>

          <div>
            <div className="mb-2 text-xs font-medium text-muted-foreground">{t("ui.color")}</div>
            <ColorSwatches value={editing.color} onChange={(c) => setEditing({ ...editing, color: c })} />
          </div>
        </div>
      )}

      <div className="space-y-2">
        {list.map((c) => (
          <div key={c.id} className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-sm">
            <CategoryIcon icon={c.icon} color={c.color} />
            <span className="font-medium">{name(c)}</span>
            {c.isSystem === 1 && <span className="text-xs text-muted-foreground">({t("ui.system")})</span>}
            {c.archived === 1 && <span className="text-xs text-destructive">({t("ui.archived")})</span>}
            <div className="ml-auto flex items-center gap-1">
              {c.archived === 0 ? (
                <>
                  <Button size="sm" variant="ghost" onClick={() => setEditing(c)}>{t("ui.edit")}</Button>
                  {c.isSystem === 0 && (
                    <>
                      <Select 
                        value={moveTo[c.id] ?? ""} onChange={(e) => setMoveTo({ ...moveTo, [c.id]: e.target.value })}>
                        <option value="">{t("ui.moveToMisc")}</option>
                        {list.filter((x) => x.id !== c.id && x.archived === 0).map((x) => (
                          <option key={x.id} value={x.id}>{name(x)}</option>
                        ))}
                      </Select>
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
