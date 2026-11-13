import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useUsers } from "@/lib/queries";
import { Button } from "@/components/ui/button";

/// Global modal shown on the host when a device asks to pair. It polls the
/// pairing hub every 3 s and blocks on the first pending request until the
/// user allows or denies it.
export function PairingPrompt() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const reqs = useQuery({ queryKey: qk.pairingRequests, queryFn: api.pairingRequests, refetchInterval: 3000 });
  const users = useUsers();
  const req = reqs.data?.requests?.[0];

  const decide = useMutation({
    mutationFn: (allow: boolean) => api.pairingDecide(req!.id, allow),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: qk.pairingRequests });
      queryClient.invalidateQueries({ queryKey: qk.syncStatus });
    },
  });

  if (!req) return null;
  const user = (users.data ?? []).find((u) => u.id === req.userId)?.name ?? req.userId;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-6">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="pairing-prompt-title"
        className="animate-fade-up w-full max-w-lg rounded-2xl border border-border bg-card p-8 shadow-[var(--card-shadow)]"
      >
        <h2 id="pairing-prompt-title" className="mb-1 font-medium">{t("pairing.title")}</h2>
        <p className="mb-4 text-sm text-muted-foreground">
          {t("pairing.text", { name: req.name, platform: req.platform, ip: req.ip, user })}
        </p>
        <div className="flex gap-2">
          <Button className="flex-1" disabled={decide.isPending} onClick={() => decide.mutate(true)}>
            {t("pairing.allow")}
          </Button>
          <Button className="flex-1" variant="outline" disabled={decide.isPending} onClick={() => decide.mutate(false)}>
            {t("pairing.deny")}
          </Button>
        </div>
        {decide.isError && <p className="mt-2 text-xs text-destructive">{String(decide.error)}</p>}
      </div>
    </div>
  );
}
