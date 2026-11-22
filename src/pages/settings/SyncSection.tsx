import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { RefreshCw } from "lucide-react";
import { api } from "@/lib/api";
import { qk, invalidateMoney } from "@/lib/queryKeys";
import { useUsers } from "@/lib/queries";
import { pairErrorText, syncErrorText } from "@/lib/syncError";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";

function localTime(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d.toLocaleString();
}

export function SyncSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const status = useQuery({ queryKey: qk.syncStatus, queryFn: api.syncStatus, refetchInterval: 10000 });
  const users = useUsers();
  const [hostUrl, setHostUrl] = useState("http://192.168.");
  const [finding, setFinding] = useState(false);
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [pairCode, setPairCode] = useState<Record<string, string>>({});
  const [syncDone, setSyncDone] = useState<string | null>(null);
  const addresses = useQuery({ queryKey: ["host-addresses"], queryFn: api.hostAddresses });
  const pairing = useQuery({ queryKey: qk.pairingRequests, queryFn: api.pairingRequests, refetchInterval: 3000 });
  const devices = useQuery({ queryKey: qk.devices, queryFn: api.devicesList, refetchInterval: 15000 });
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
  const syncNow = useMutation({
    mutationFn: api.syncNow,
    onSuccess: (r) => {
      setSyncDone(
        r.mode === "host"
          ? t("ui.refreshed")
          : r.pushed || r.pulled
            ? t("ui.syncResult", { pushed: r.pushed ?? 0, pulled: r.pulled ?? 0 })
            : t("ui.syncResultNone"),
      );
      void queryClient.invalidateQueries({ queryKey: qk.syncStatus });
      // Pulled changes may touch any entity — refresh data views too.
      invalidateMoney(queryClient);
    },
  });
  const s = status.data;
  // While status loads the host view is safer: it only shows local info.
  const isClient = s?.mode === "client";
  const lastSync = localTime(s?.lastSyncAt);

  return (
    <div className="space-y-4 text-sm">
      <div>
        <div className="flex items-center justify-between gap-2">
          <div className="mb-1 font-medium">
            {isClient ? t("ui.clientMode") : t("ui.hostMode")}
          </div>
          <Button size="sm" variant="outline" loading={syncNow.isPending} onClick={() => syncNow.mutate()}>
            <RefreshCw className="h-3.5 w-3.5" />
            {t("ui.syncNow")}
          </Button>
        </div>
        {!isClient && <p className="mb-1 text-xs text-muted-foreground">{t("ui.hostDbHint")}</p>}
        {isClient && s?.hostUrl && (
          <code className="mb-1 block text-sm font-semibold">{s.hostUrl}</code>
        )}
        {isClient && (
          <p className="mb-1 text-xs text-muted-foreground">
            {lastSync ? t("ui.lastSyncAt", { time: lastSync }) : t("ui.neverSynced")}
          </p>
        )}
        {syncNow.isError && (
          <p className="mb-1 text-xs text-destructive">{syncErrorText(t, String(syncNow.error))}</p>
        )}
        {syncDone && !syncNow.isPending && !syncNow.isError && (
          <p className="mb-1 text-xs text-muted-foreground">{syncDone}</p>
        )}
        {!isClient && (
          <>
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
            {(devices.data ?? []).length > 0 && (
              <div className="mt-2">
                <div className="text-xs text-muted-foreground">{t("ui.devicesList")}:</div>
                {(devices.data ?? []).map((d) => (
                  <div key={d.id} className="text-sm">
                    {d.name}
                    <span className="text-muted-foreground"> · {d.platform}</span>
                    {d.id === s?.deviceId
                      ? <span className="text-muted-foreground"> · {t("ui.thisDevice")}</span>
                      : localTime(d.lastSeenAt) && <span className="text-muted-foreground"> · {t("ui.lastSeen", { time: localTime(d.lastSeenAt) })}</span>}
                  </div>
                ))}
              </div>
            )}
          </>
        )}
        <p className="mt-1 text-xs text-muted-foreground">
          {t("ui.syncStatus")}: {s?.pairedDevices ?? 0} · {s?.pendingOps ?? 0} {t("ui.pendingOps")}
        </p>
        {s?.lastError && (
          <p className="mt-1 text-xs text-destructive">{syncErrorText(t, s.lastError)}</p>
        )}
      </div>
      {!isClient && (
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
        </div>
      )}
      <ConflictsBlock />
    </div>
  );
}

interface ConflictItem {
  id: string;
  entity: string;
  entityId: string;
  local: string;
  server: string;
}

function ConflictsBlock() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: qk.conflicts, queryFn: api.conflictList, refetchInterval: 15000 });
  const resolve = useMutation({
    // Resolving one row also closes its duplicates — the host marks every
    // open conflict on the same entity_id resolved, and locally we do the
    // same for the ids we know about.
    mutationFn: async ({ group, winner }: { group: ConflictItem[]; winner: "local" | "server" }) => {
      for (const c of group) {
        await api.conflictResolve(c.id, winner);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: qk.conflicts });
      void queryClient.invalidateQueries({ queryKey: qk.syncStatus });
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
  // Thousands of stale duplicates once shipped as one card each — group by
  // the entity they fight over and resolve the whole group in one click.
  const groups = new Map<string, ConflictItem[]>();
  for (const c of list.data) {
    const k = `${c.entity}|${c.entityId}`;
    const g = groups.get(k) ?? [];
    g.push(c);
    groups.set(k, g);
  }
  const entries = [...groups.entries()];
  const shown = entries.slice(0, 20);
  return (
    <div>
      <div className="mb-1 font-medium text-destructive">{t("ui.conflicts")}: {list.data.length}</div>
      {shown.map(([key, group]) => {
        const c = group[0];
        return (
          <div key={key} className="mb-2 space-y-1 rounded-xl border border-destructive/40 bg-card p-3">
            <div className="text-xs text-muted-foreground">
              {t(`ui.entity.${c.entity}`, { defaultValue: c.entity })}
              {group.length > 1 && ` ×${group.length}`} · {t("ui.conflictHint")}
            </div>
            <div className="flex items-center gap-2">
              <span className="flex-1 truncate">{t("ui.variantA")}: {short(c.local)}</span>
              <Button size="sm" onClick={() => resolve.mutate({ group, winner: "local" })}>{t("ui.keepA")}</Button>
              <span className="flex-1 truncate">{t("ui.variantB")}: {short(c.server)}</span>
              <Button size="sm" variant="outline" onClick={() => resolve.mutate({ group, winner: "server" })}>{t("ui.keepB")}</Button>
            </div>
          </div>
        );
      })}
      {entries.length > shown.length && (
        <p className="text-xs text-muted-foreground">{t("ui.conflictsMore", { count: entries.length - shown.length })}</p>
      )}
    </div>
  );
}
