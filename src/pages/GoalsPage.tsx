import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { formatMinor, parseMinor } from "@/lib/money";

interface GoalRow {
  id: string; name: string; targetMinor: number; currentMinor: number;
  currency: string; deadline?: string | null; color: string; archived: number;
}

export function GoalsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["goals"], queryFn: () => api.entityList<GoalRow>("goals", true) });
  const [editing, setEditing] = useState<Partial<GoalRow> | null>(null);
  const [targetStr, setTargetStr] = useState("");
  const [currentStr, setCurrentStr] = useState("");

  const invalidate = () => qc.invalidateQueries({ queryKey: ["goals"] });
  const save = useMutation({
    mutationFn: (g: Record<string, unknown>) => api.goalUpsert(g),
    onSuccess: () => { invalidate(); setEditing(null); },
  });
  const del = useMutation({ mutationFn: (id: string) => api.entityDelete("goals", id), onSuccess: invalidate });
  const restore = useMutation({ mutationFn: (id: string) => api.entityRestore("goals", id), onSuccess: invalidate });

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
        <Button onClick={() => open()}>+ {t("ui.add")}</Button>
      </div>

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.name")}
              value={editing.name ?? ""} onChange={(e) => setEditing({ ...editing, name: e.target.value })} autoFocus />
            <input type="date" className="rounded border border-border bg-background px-3 py-2 text-sm"
              value={editing.deadline ?? ""} onChange={(e) => setEditing({ ...editing, deadline: e.target.value || null })} />
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.goalTarget")}
              inputMode="decimal" value={targetStr} onChange={(e) => setTargetStr(e.target.value)} />
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.goalCurrent")}
              inputMode="decimal" value={currentStr} onChange={(e) => setCurrentStr(e.target.value)} />
            <select className="rounded border border-border bg-background px-3 py-2 text-sm"
              value={editing.currency ?? "BYN"} onChange={(e) => setEditing({ ...editing, currency: e.target.value })}>
              {["BYN", "USD", "EUR", "RUB"].map((c) => <option key={c}>{c}</option>)}
            </select>
            <input type="color" value={editing.color ?? "#059669"} onChange={(e) => setEditing({ ...editing, color: e.target.value })} />
          </div>
          <div className="flex gap-2">
            <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
          </div>
        </div>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        {(list.data ?? []).map((g) => {
          const pct = g.targetMinor > 0 ? Math.min(100, Math.round((g.currentMinor / g.targetMinor) * 100)) : 0;
          return (
            <div key={g.id} className="rounded-2xl border border-border bg-card p-4">
              <div className="mb-1 flex items-center gap-2">
                <span className="h-3 w-3 rounded-full" style={{ background: g.color }} />
                <span className="font-medium">{g.name}</span>
                {g.deadline && <span className="ml-auto text-xs text-muted-foreground">{t("ui.till")} {g.deadline}</span>}
              </div>
              <div className="mb-2 h-2 rounded-full bg-muted">
                <div className="h-2 rounded-full bg-primary" style={{ width: `${pct}%` }} />
              </div>
              <div className="flex items-center text-sm">
                <span>{formatMinor(g.currentMinor, g.currency)} / {formatMinor(g.targetMinor, g.currency)} · {pct}%</span>
                <div className="ml-auto flex gap-1">
                  {g.archived === 0 ? (
                    <>
                      <Button size="sm" variant="ghost" onClick={() => open(g)}>{t("ui.edit")}</Button>
                      <Button size="sm" variant="ghost" onClick={() => del.mutate(g.id)}>{t("ui.archive")}</Button>
                    </>
                  ) : (
                    <Button size="sm" variant="ghost" onClick={() => restore.mutate(g.id)}>{t("ui.restore")}</Button>
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
