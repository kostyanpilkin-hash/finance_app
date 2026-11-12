import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { DbUnlock } from "@/components/DbUnlock";

/** Startup failure screen: the db couldn't be opened at all. Offers staged
 * backup restore, the data folder, the recovery-key flow and a relaunch. */
export function DbError() {
  const { t } = useTranslation();
  const [showUnlock, setShowUnlock] = useState(false);
  const info = useQuery({ queryKey: ["startup-error"], queryFn: api.startupError });
  const backups = useQuery({ queryKey: ["backups"], queryFn: api.backupList });
  const restore = useMutation({
    mutationFn: (name: string) => api.backupRestore(name),
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 700));
      await relaunch();
    },
  });

  if (showUnlock) {
    return <DbUnlock onBack={() => setShowUnlock(false)} />;
  }

  return (
    <div className="grid min-h-screen place-items-center bg-[var(--bg)] p-6">
      <div className="animate-fade-up w-full max-w-lg rounded-2xl border border-border bg-card p-8 shadow-[var(--card-shadow)]">
        <h1 className="font-display mb-1 text-3xl font-semibold text-primary">{t("app.title")}</h1>
        <h2 className="mb-1 font-medium">{t("dberror.title")}</h2>
        <p className="mb-4 text-sm text-muted-foreground">{t("dberror.hint")}</p>
        {info.data?.message && (
          <details className="mb-4 rounded-lg bg-muted px-3 py-2 text-xs">
            <summary className="cursor-pointer text-muted-foreground">{t("dberror.details")}</summary>
            <pre className="mt-2 whitespace-pre-wrap break-all">{info.data.message}</pre>
          </details>
        )}

        <div className="mb-4">
          <div className="mb-2 text-sm font-medium">{t("dberror.backups")}</div>
          <div className="space-y-1">
            {(backups.data ?? []).slice(0, 8).map((b) => (
              <div key={b} className="flex items-center justify-between rounded-lg bg-muted px-3 py-1.5 font-mono text-xs">
                <span>{b}</span>
                <button
                  className="text-primary hover:underline"
                  disabled={restore.isPending || restore.isSuccess}
                  onClick={() => {
                    if (window.confirm(t("dberror.restoreConfirm", { name: b }))) restore.mutate(b);
                  }}
                >
                  {t("dberror.restore")}
                </button>
              </div>
            ))}
            {backups.data?.length === 0 && (
              <p className="text-xs text-muted-foreground">{t("dberror.noBackups")}</p>
            )}
          </div>
          {restore.isSuccess && <p className="mt-2 text-xs text-primary">{t("ui.restoreRestarting")}</p>}
          {restore.isError && <p className="mt-2 text-xs text-destructive">{String(restore.error)}</p>}
        </div>

        <div className="flex flex-col gap-2">
          <Button
            variant="outline"
            onClick={() => info.data?.dataDir && void revealItemInDir(info.data.dataDir)}
            disabled={!info.data?.dataDir}
          >
            {t("dberror.openFolder")}
          </Button>
          <Button variant="outline" onClick={() => setShowUnlock(true)}>
            {t("dberror.recoveryKey")}
          </Button>
          <Button onClick={() => void relaunch()}>{t("dberror.retry")}</Button>
        </div>
      </div>
    </div>
  );
}
