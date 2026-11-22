import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { api } from "@/lib/api";
import { qk, NON_DATA_ROOTS } from "@/lib/queryKeys";
import { Button } from "@/components/ui/button";

/**
 * Manual page refresh for the heading row. On a client it first runs one
 * sync round so changes from other devices arrive, then refetches every
 * db-backed query — the same invalidation data-changed events use. The
 * window is never reloaded, so filters and scroll position survive.
 */
export function RefreshButton() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const status = useQuery({ queryKey: qk.syncStatus, queryFn: api.syncStatus, staleTime: 30_000 });
  const [busy, setBusy] = useState(false);

  const onClick = async () => {
    if (busy) return;
    setBusy(true);
    try {
      if (status.data?.mode === "client") {
        try {
          await api.syncNow();
        } catch {
          // Sync failures are explained in Settings → Devices; local data still refreshes.
        }
      }
      await qc.invalidateQueries({
        predicate: (q) => !NON_DATA_ROOTS.has(String(q.queryKey[0])),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Button
      size="icon-sm"
      variant="ghost"
      onClick={onClick}
      disabled={busy}
      aria-label={t("ui.refresh")}
      title={t("ui.refresh")}
    >
      <RefreshCw className={`h-4 w-4 ${busy ? "animate-spin" : ""}`} />
    </Button>
  );
}
