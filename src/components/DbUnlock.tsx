import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";

/// Shown when the db file exists but its key is missing from the OS keyring
/// (fresh OS, new machine, migrated backup). The user types the recovery key
/// shown at the original install; on success the app relaunches unlocked.
export function DbUnlock({ onBack }: { onBack?: () => void }) {
  const { t } = useTranslation();
  const [key, setKey] = useState("");

  const unlock = useMutation({
    mutationFn: () => api.unlockDb(key.trim()),
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 700));
      await relaunch();
    },
  });

  return (
    <div className="grid min-h-screen place-items-center bg-[var(--bg)] p-6">
      <div className="animate-fade-up w-full max-w-lg rounded-2xl border border-border bg-card p-8 shadow-[var(--card-shadow)]">
        <h1 className="font-display mb-1 text-3xl font-semibold text-primary">{t("app.title")}</h1>
        <h2 className="mb-1 font-medium">{t("dblocked.title")}</h2>
        <p className="mb-4 text-sm text-muted-foreground">{t("dblocked.hint")}</p>
        <input
          className="mb-4 w-full rounded-[var(--input-radius)] border border-border bg-background px-3 py-2 font-mono text-sm tracking-wider"
          placeholder="K3F4-9XM2-…"
          value={key}
          autoFocus
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && key.trim() && !unlock.isPending && unlock.mutate()}
        />
        <Button
          className="w-full"
          disabled={!key.trim() || unlock.isPending || unlock.isSuccess}
          onClick={() => unlock.mutate()}
        >
          {unlock.isSuccess ? t("dblocked.restarting") : unlock.isPending ? t("dblocked.checking") : t("ui.unlock")}
        </Button>
        {unlock.isError && <p className="mt-2 text-sm text-destructive">{t("dblocked.wrong")}</p>}
        {onBack && (
          <Button variant="ghost" className="mt-3 w-full" onClick={onBack}>
            {t("onboarding.back")}
          </Button>
        )}
      </div>
    </div>
  );
}
