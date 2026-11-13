import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useUsers } from "@/lib/queries";
import { pairErrorText, syncErrorText } from "@/lib/syncError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";

export function SyncSection() {
  const { t } = useTranslation();
  const status = useQuery({ queryKey: qk.syncStatus, queryFn: api.syncStatus, refetchInterval: 10000 });
  const users = useUsers();
  const [hostUrl, setHostUrl] = useState("http://192.168.");
  const [finding, setFinding] = useState(false);
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [pairCode, setPairCode] = useState<Record<string, string>>({});
  const addresses = useQuery({ queryKey: ["host-addresses"], queryFn: api.hostAddresses });
  const pairing = useQuery({ queryKey: qk.pairingRequests, queryFn: api.pairingRequests, refetchInterval: 3000 });
  const pairingLocked = pairing.data?.locked === true;
  const pair = useMutation({
    mutationFn: () => api.pairToHost(hostUrl, code, name),
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 1200));
      await relaunch();
    },
  });
  const makeCode = useMutation({
    mutationFn: (uid: string) => api.devicePairCode(uid),
    onSuccess: (c, uid) => setPairCode((s) => ({ ...s, [uid]: c })),
  });
  const s = status.data;
  return (
    <div className="space-y-4 text-sm">
      <div>
        <div className="mb-1 font-medium">{t("ui.hostMode")}</div>
        {(addresses.data ?? []).length > 0 && (
          <div className="mb-2">
            <div className="text-xs text-muted-foreground">{t("ui.hostAddress")}:</div>
            {(addresses.data ?? []).map((a, i) => (
              <code key={a} className={`block text-sm ${i === 0 ? "font-semibold" : "text-muted-foreground"}`}>{a}</code>
            ))}
          </div>
        )}
        {(users.data ?? []).map((u) => (
          <div key={u.id} className="flex items-center gap-3">
            <span className="w-32">{u.name}</span>
            {pairCode[u.id] && !pairingLocked ? (
              <code className="rounded bg-muted px-2 py-1 text-lg font-semibold tracking-widest">{pairCode[u.id]}</code>
            ) : (
              <Button size="sm" variant="outline" onClick={() => makeCode.mutate(u.id)}>{t("ui.pairDevice")}</Button>
            )}
          </div>
        ))}
        <p className="mt-1 text-xs text-muted-foreground">{t("pairing.hint")}</p>
        {pairingLocked && <p className="mt-1 text-xs text-destructive">{t("pairing.locked")}</p>}
        <p className="mt-1 text-xs text-muted-foreground">
          {t("ui.syncStatus")}: {s?.pairedDevices ?? 0} · {s?.pendingOps ?? 0} {t("ui.pendingOps")}
        </p>
        {s?.lastError && (
          <p className="mt-1 text-xs text-destructive">{syncErrorText(t, s.lastError)}</p>
        )}
      </div>
      <div>
        <div className="mb-1 font-medium">{t("ui.joinHost")}</div>
        <div className="flex flex-wrap items-end gap-2">
          <Field label={t("ui.hostUrl")} className="min-w-48 flex-1">
            <Input placeholder="http://192.168.1.10:45322"
              value={hostUrl} onChange={(e) => setHostUrl(e.target.value)} />
          </Field>
          <Field label={t("ui.code")} className="w-24">
            <Input value={code} onChange={(e) => setCode(e.target.value)} />
          </Field>
          <Field label={t("ui.deviceName")} className="w-40">
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <Button size="sm" variant="outline" loading={finding} onClick={async () => {
            setFinding(true);
            try {
              const hosts = await api.discoverHosts();
              if (hosts[0]?.url) setHostUrl(hosts[0].url);
            } finally {
              setFinding(false);
            }
          }}>{t("ui.findHost")}</Button>
          <Button size="sm" onClick={() => pair.mutate()} disabled={pair.isPending}>{t("ui.connect")}</Button>
        </div>
        {pair.isPending && <p className="mt-1 text-xs text-muted-foreground">{t("pairing.waiting")}</p>}
        {pair.isError && <p className="mt-1 text-destructive">{pairErrorText(t, String(pair.error))}</p>}
        {pair.isSuccess && <p className="mt-1 text-xs text-muted-foreground">{t("ui.restoreRestart")}</p>}
        {s?.hostUrl && <p className="mt-1 text-xs text-muted-foreground">{s.hostUrl} · seq {s.lastSeq}</p>}
      </div>
      <ConflictsBlock />
    </div>
  );
}

function ConflictsBlock() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: qk.conflicts, queryFn: api.conflictList, refetchInterval: 15000 });
  const resolve = useMutation({
    mutationFn: ({ id, winner }: { id: string; winner: "local" | "server" }) => api.conflictResolve(id, winner),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: qk.conflicts });
      queryClient.invalidateQueries({ queryKey: qk.syncStatus });
    },
  });
  if (!list.data?.length) return null;
  const short = (json: string) => {
    try {
      const v = JSON.parse(json);
      const name = v.name ?? v.amountMinor ?? v.payload?.name ?? v.payload?.amountMinor ?? "";
      return name !== "" ? String(name) : JSON.stringify(v).slice(0, 60);
    } catch {
      return json.slice(0, 60);
    }
  };
  return (
    <div>
      <div className="mb-1 font-medium text-destructive">{t("ui.conflicts")}: {list.data.length}</div>
      {list.data.map((c) => (
        <div key={c.id} className="mb-2 space-y-1 rounded-xl border border-destructive/40 bg-card p-3">
          <div className="text-xs text-muted-foreground">{c.entity} · {t("ui.conflictHint")}</div>
          <div className="flex items-center gap-2">
            <span className="flex-1 truncate">{t("ui.variantA")}: {short(c.local)}</span>
            <Button size="sm" onClick={() => resolve.mutate({ id: c.id, winner: "local" })}>{t("ui.keepA")}</Button>
            <span className="flex-1 truncate">{t("ui.variantB")}: {short(c.server)}</span>
            <Button size="sm" variant="outline" onClick={() => resolve.mutate({ id: c.id, winner: "server" })}>{t("ui.keepB")}</Button>
          </div>
        </div>
      ))}
    </div>
  );
}
