import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { Button } from "@/components/ui/button";
import { useUpdateStore } from "@/stores/updateStore";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";

export function SecuritySection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const enabled = useQuery({ queryKey: qk.pinEnabled, queryFn: api.pinEnabled });
  const [pin, setPin] = useState("");
  const set_ = useMutation({
    mutationFn: () => api.pinSet(pin),
    onSuccess: () => { setPin(""); qc.invalidateQueries({ queryKey: qk.pinEnabled }); },
  });
  const clear = useMutation({
    mutationFn: api.pinClear,
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.pinEnabled }),
  });
  return (
    <div className="space-y-3 text-sm">
      {enabled.data ? (
        <div className="flex items-center gap-3">
          <span>{t("ui.pinOn")}</span>
          <Button size="sm" variant="outline" onClick={() => clear.mutate()}>{t("ui.pinOff")}</Button>
        </div>
      ) : (
        <div className="flex items-end gap-2">
          <Field label={t("ui.pin")} className="w-36">
            <Input type="password" inputMode="numeric"
              maxLength={8} placeholder="4–8" value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))} />
          </Field>
          <Button size="sm" onClick={() => set_.mutate()} disabled={pin.length < 4 || set_.isPending}>{t("ui.setPin")}</Button>
        </div>
      )}
      <p className="text-xs text-muted-foreground">{t("ui.pinHint")}</p>
    </div>
  );
}

export function BackupSection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: qk.backups, queryFn: api.backupList });
  const create = useMutation({
    mutationFn: api.backupCreate,
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.backups }),
  });
  const restore = useMutation({
    mutationFn: api.backupRestore,
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 700));
      await relaunch();
    },
  });
  return (
    <div className="space-y-3 text-sm">
      <div className="flex items-center gap-3">
        <Button size="sm" onClick={() => create.mutate()} loading={create.isPending}>{t("ui.backupNow")}</Button>
        {create.data && <span className="text-xs text-muted-foreground">{create.data}</span>}
      </div>
      <p className="text-xs text-muted-foreground">{t("ui.backupHint")}</p>
      <div className="space-y-1">
        {(list.data ?? []).slice(0, 12).map((b) => (
          <div key={b} className="flex items-center justify-between rounded-lg bg-muted px-3 py-1.5 font-mono text-xs">
            <span>{b}</span>
            <button className="text-primary hover:underline" onClick={() => {
              if (window.confirm(t("ui.restoreConfirm"))) restore.mutate(b);
            }}>{t("ui.restore")}</button>
          </div>
        ))}
        {restore.isSuccess && <p className="text-xs text-primary">{t("ui.restoreRestarting")}</p>}
        {restore.isError && <p className="text-xs text-destructive">{String(restore.error)}</p>}
        {list.data?.length === 0 && <p className="text-xs text-muted-foreground">{t("ui.noBackups")}</p>}
      </div>
    </div>
  );
}

export function UpdateCheckButton() {
  const { t } = useTranslation();
  const { phase, checkNow } = useUpdateStore();
  return (
    <span className="flex items-center gap-2">
      <Button size="sm" variant="outline" onClick={() => void checkNow(true)} disabled={phase === "checking" || phase === "downloading"}>
        {phase === "checking" ? t("update.checking") : t("update.check")}
      </Button>
      {phase === "uptodate" && <span className="text-xs text-income">{t("update.upToDate")}</span>}
    </span>
  );
}
