import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useUsers } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Select } from "@/components/ui/select";
import { Field } from "@/components/ui/field";
import { useLocalSettings } from "./useLocalSettings";

export function TgSection() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [token, setToken] = useState("");
  const status = useQuery({ queryKey: qk.tgStatus, queryFn: api.tgStatus, refetchInterval: 15000 });
  const users = useUsers();
  const [code, setCode] = useState<Record<string, string>>({});
  const connect = useMutation({
    mutationFn: () => api.tgConnect(token),
    onSuccess: () => { setToken(""); queryClient.invalidateQueries({ queryKey: qk.tgStatus }); },
  });
  const disconnect = useMutation({
    mutationFn: api.tgDisconnect,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: qk.tgStatus }),
  });
  const pair = useMutation({
    mutationFn: (userId: string) => api.tgPairCode(userId),
    onSuccess: (c, userId) => setCode((s) => ({ ...s, [userId]: c })),
  });
  const s = status.data;
  const stamp = (iso?: string | null) => (iso ? new Date(iso).toLocaleString(i18n.language) : "—");
  const health = s?.health ?? "stopped";
  return (
    <div className="space-y-3 text-sm">
      {s?.enabled && (
        <div className="space-y-1 rounded-xl bg-muted/60 p-3">
          <div className="flex items-center gap-2 font-medium">
            <span className={cn("h-2.5 w-2.5 rounded-full",
              health === "ok" ? "bg-income" : health === "offline" ? "bg-amber-500" : "bg-muted-foreground")} />
            {t(`settings.bot.status.${health}`)}
          </div>
          <div className="text-xs text-muted-foreground">{t("settings.bot.lastMessage")}: {stamp(s.status?.lastMessage)}</div>
          {s.status?.lastError && (
            <div className="text-xs text-destructive">
              {t("settings.bot.lastError")} ({stamp(s.status.lastErrorAt)}): {s.status.lastError}
            </div>
          )}
          {s.logPath && <div className="break-all text-xs text-muted-foreground">{t("settings.bot.log")}: {s.logPath}</div>}
        </div>
      )}
      {!s?.enabled ? (
        <div className="space-y-2">
          <Field label={t("ui.tgToken")}>
            <Input type="password"
              placeholder="Bot token (BotFather)" value={token} onChange={(e) => setToken(e.target.value)} />
          </Field>
          <Button size="sm" onClick={() => connect.mutate()} disabled={!token.trim() || connect.isPending}>
            {t("ui.connect")}
          </Button>
          {connect.isError && <p className="text-destructive">{String(connect.error)}</p>}
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3">
            <span>@{s.username} — {t("ui.connected")}</span>
            <Button size="sm" variant="outline" onClick={() => disconnect.mutate()}>{t("ui.disconnect")}</Button>
          </div>
          <div className="space-y-2">
            {(users.data ?? []).map((u) => {
              const linked = s.linked?.some(([uid]) => uid === u.id);
              return (
                <div key={u.id} className="flex items-center gap-3">
                  <span className="w-32">{u.name}</span>
                  {linked ? (
                    <span className="text-income">{t("ui.tgLinked")}</span>
                  ) : code[u.id] ? (
                    <code className="rounded bg-muted px-2 py-1 text-lg font-semibold tracking-widest">{code[u.id]}</code>
                  ) : (
                    <Button size="sm" variant="outline" onClick={() => pair.mutate(u.id)}>{t("ui.tgPair")}</Button>
                  )}
                </div>
              );
            })}
            {pair.isPending === false && Object.keys(code).length > 0 && (
              <p className="text-xs text-muted-foreground">{t("ui.tgPairHint")}</p>
            )}
          </div>
        </>
      )}
    </div>
  );
}

export function BotNotifySection() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  const input = "h-9 w-auto";
  const toggle = (key: string, fallback: string, label: string) => (
    <Checkbox className="min-w-52 font-medium" checked={get(key, fallback) === "1"} label={label}
      onCheckedChange={(v) => set(key, v ? "1" : "0")} />
  );
  const time = (key: string, fallback: string) => (
    <Input type="time" className={input} value={get(key, fallback)}
      onChange={(e) => e.target.value && set(key, e.target.value)} />
  );
  const row = (children: React.ReactNode, hint?: string) => (
    <div className="space-y-1 border-b border-border/50 pb-3 last:border-0">
      <div className="flex flex-wrap items-center gap-3">{children}</div>
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
  return (
    <div className="space-y-3 text-sm">
      {row(<>{toggle("bot_daily_enabled", "1", t("settings.bot.daily"))}<span>{t("settings.bot.at")}</span>{time("bot_daily_time", "23:00")}</>)}
      {row(<>
        {toggle("bot_weekly_enabled", "0", t("settings.bot.weekly"))}
        <Select className={input} value={get("bot_weekly_day", "7")} onChange={(e) => set("bot_weekly_day", e.target.value)}>
          {[1, 2, 3, 4, 5, 6, 7].map((d) => <option key={d} value={d}>{t(`ui.wd.${d}`)}</option>)}
        </Select>
        <span>{t("settings.bot.at")}</span>{time("bot_weekly_time", "20:00")}
      </>, t("settings.bot.weeklyHint"))}
      {row(<>
        {toggle("bot_monthly_enabled", "0", t("settings.bot.monthly"))}
        <Select className={input} value={get("bot_monthly_day", "1")} onChange={(e) => set("bot_monthly_day", e.target.value)}>
          {Array.from({ length: 31 }, (_, i) => i + 1).map((d) => <option key={d} value={d}>{d}</option>)}
        </Select>
        <span>{t("settings.bot.day")}</span>
        <span>{t("settings.bot.at")}</span>{time("bot_monthly_time", "20:00")}
      </>, t("settings.bot.monthlyHint"))}
      {row(<>{toggle("bot_reminders_enabled", "1", t("settings.bot.reminders"))}<span>{t("settings.bot.at")}</span>{time("bot_reminders_time", "10:00")}</>,
        t("settings.bot.remindersHint"))}
      {row(<>
        {toggle("bot_quiet_enabled", "0", t("settings.bot.quiet"))}
        <span>{t("settings.bot.from")}</span>{time("bot_quiet_from", "23:30")}
        <span>{t("settings.bot.to")}</span>{time("bot_quiet_to", "08:00")}
      </>, t("settings.bot.quietHint"))}
    </div>
  );
}
