import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "@/lib/api";
import { pairErrorText } from "@/lib/syncError";
import { LANGUAGES } from "@/i18n";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";
import { Checkbox } from "@/components/ui/checkbox";

type Step = "language" | "mode" | "join" | "users" | "accounts" | "key";

export function Onboarding() {
  const { t } = useTranslation();
  const { language, setLanguage } = useSettingsStore();
  const queryClient = useQueryClient();
  const [step, setStep] = useState<Step>("language");
  const [names, setNames] = useState<string[]>([""]);
  const [current, setCurrent] = useState(0);
  const [starter, setStarter] = useState<string[]>(["cash"]);
  const [savedKey, setSavedKey] = useState(false);
  const [hostUrl, setHostUrl] = useState("http://");
  const [joinCode, setJoinCode] = useState("");
  const [deviceName, setDeviceName] = useState("");

  const recKey = useQuery({ queryKey: ["recovery-key"], queryFn: api.recoveryKey });
  const hosts = useQuery({
    queryKey: ["discover-hosts"],
    queryFn: api.discoverHosts,
    enabled: step === "join",
    retry: false,
    refetchOnWindowFocus: false,
  });

  const finish = useMutation({
    mutationFn: () =>
      api.completeOnboarding({
        userNames: names.filter((n) => n.trim()),
        currentUser: Math.min(current, names.filter((n) => n.trim()).length - 1),
        starterAccounts: starter,
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["onboarding"] }),
  });

  const join = useMutation({
    mutationFn: () => api.joinHost(hostUrl.trim(), joinCode.trim(), deviceName.trim() || "device"),
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 1200));
      await relaunch();
    },
  });

  const toggle = (kind: string) =>
    setStarter((s) => (s.includes(kind) ? s.filter((k) => k !== kind) : [...s, kind]));

  return (
    <div className="grid min-h-screen place-items-center bg-[var(--bg)] p-6">
      <div className="animate-fade-up w-full max-w-lg rounded-2xl border border-border bg-card p-8 shadow-[var(--card-shadow)]">
        <h1 className="font-display mb-1 text-3xl font-semibold text-primary">{t("app.title")}</h1>
        <p className="mb-6 text-sm text-muted-foreground">{t("onboarding.subtitle")}</p>

        {step === "language" && (
          <div>
            <h2 className="mb-3 font-medium">{t("onboarding.language")}</h2>
            <div className="mb-6 flex flex-wrap gap-2">
              {LANGUAGES.map((l) => (
                <Button key={l.id} variant={language === l.id ? "default" : "outline"} onClick={() => setLanguage(l.id)}>
                  {l.label}
                </Button>
              ))}
            </div>
            <Button className="w-full" onClick={() => setStep("mode")}>{t("onboarding.next")}</Button>
          </div>
        )}

        {step === "mode" && (
          <div>
            <h2 className="mb-3 font-medium">{t("onboarding.modeTitle")}</h2>
            <button
              className="mb-2 w-full rounded-lg border border-border p-3 text-left hover:border-primary"
              onClick={() => setStep("users")}
            >
              <div className="font-medium">{t("onboarding.modeNew")}</div>
              <div className="text-xs text-muted-foreground">{t("onboarding.modeNewHint")}</div>
            </button>
            <button
              className="w-full rounded-lg border border-border p-3 text-left hover:border-primary"
              onClick={() => setStep("join")}
            >
              <div className="font-medium">{t("onboarding.modeJoin")}</div>
              <div className="text-xs text-muted-foreground">{t("onboarding.modeJoinHint")}</div>
            </button>
            <Button className="mt-6 w-full" variant="outline" onClick={() => setStep("language")}>
              {t("onboarding.back")}
            </Button>
          </div>
        )}

        {step === "join" && (
          <div>
            <h2 className="mb-1 font-medium">{t("onboarding.joinTitle")}</h2>
            <p className="mb-3 text-sm text-muted-foreground">{t("onboarding.joinHint")}</p>
            {hosts.isPending && <p className="mb-2 text-xs text-muted-foreground">{t("onboarding.joinSearch")}</p>}
            {(hosts.data ?? []).map((h) => (
              <button
                key={h.url}
                className={`mb-2 w-full rounded-lg border p-2 text-left text-sm ${
                  hostUrl === h.url ? "border-primary" : "border-border"
                }`}
                onClick={() => setHostUrl(h.url)}
              >
                {h.host} · {h.url}
              </button>
            ))}
            {hosts.isSuccess && hosts.data.length === 0 && (
              <p className="mb-2 text-xs text-muted-foreground">{t("onboarding.joinNone")}</p>
            )}
            <Field label={t("ui.hostUrl")} className="mb-2">
              <Input
                className="w-full"
                placeholder={t("onboarding.joinUrl")}
                value={hostUrl}
                onChange={(e) => setHostUrl(e.target.value)}
              />
            </Field>
            <div className="mb-4 flex gap-2">
              <Field label={t("onboarding.joinCode")} className="w-28">
                <Input
                  className="tracking-widest"
                  value={joinCode}
                  onChange={(e) => setJoinCode(e.target.value)}
                />
              </Field>
              <Field label={t("onboarding.joinName")} className="flex-1">
                <Input
                  value={deviceName}
                  onChange={(e) => setDeviceName(e.target.value)}
                />
              </Field>
            </div>
            <div className="flex gap-2">
              <Button variant="outline" onClick={() => setStep("mode")}>{t("onboarding.back")}</Button>
              <Button
                className="flex-1"
                disabled={!hostUrl.includes(":") || !joinCode.trim() || join.isPending || join.isSuccess}
                onClick={() => join.mutate()}
              >
                {join.isSuccess
                  ? t("onboarding.joinDone")
                  : join.isPending
                    ? t("onboarding.joinWorking")
                    : t("onboarding.joinBtn")}
              </Button>
            </div>
            {join.isPending && <p className="mt-2 text-sm text-muted-foreground">{t("pairing.waiting")}</p>}
            {join.isError && (
              <p className="mt-2 text-sm text-destructive">
                {pairErrorText(t, String(join.error))}
              </p>
            )}
          </div>
        )}

        {step === "users" && (
          <div>
            <h2 className="mb-1 font-medium">{t("onboarding.usersTitle")}</h2>
            <p className="mb-3 text-sm text-muted-foreground">{t("onboarding.usersHint")}</p>
            {names.map((n, i) => (
              <div key={i} className="mb-2 flex items-center gap-2">
                <Input
                  className="flex-1 text-sm"
                  placeholder={i === 0 ? t("onboarding.yourName") : t("onboarding.otherName")}
                  value={n}
                  autoFocus={i === names.length - 1}
                  onChange={(e) => setNames((ns) => ns.map((x, j) => (j === i ? e.target.value : x)))}
                />
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  <input type="radio" checked={current === i} onChange={() => setCurrent(i)} />
                  {t("onboarding.thisIsMe")}
                </label>
                {i > 0 && (
                  <Button size="sm" variant="ghost" onClick={() => setNames((ns) => ns.filter((_, j) => j !== i))}>×</Button>
                )}
              </div>
            ))}
            <Button size="sm" variant="outline" onClick={() => setNames((ns) => [...ns, ""])}>
              + {t("onboarding.addUser")}
            </Button>
            <div className="mt-6 flex gap-2">
              <Button variant="outline" onClick={() => setStep("mode")}>{t("onboarding.back")}</Button>
              <Button className="flex-1" disabled={!names.some((n) => n.trim())} onClick={() => setStep("accounts")}>
                {t("onboarding.next")}
              </Button>
            </div>
          </div>
        )}

        {step === "accounts" && (
          <div>
            <h2 className="mb-1 font-medium">{t("onboarding.accountsTitle")}</h2>
            <p className="mb-3 text-sm text-muted-foreground">{t("onboarding.accountsHint")}</p>
            {(["cash", "savings"] as const).map((k) => (
              <div key={k} className="mb-2 rounded-lg border border-border px-3">
                <Checkbox checked={starter.includes(k)} label={t(`onboarding.account.${k}`)} onCheckedChange={() => toggle(k)} />
              </div>
            ))}
            <div className="mt-6 flex gap-2">
              <Button variant="outline" onClick={() => setStep("users")}>{t("onboarding.back")}</Button>
              <Button className="flex-1" onClick={() => setStep(recKey.data ? "key" : "key")}>{t("onboarding.next")}</Button>
            </div>
          </div>
        )}

        {step === "key" && (
          <div>
            {recKey.data ? (
              <>
                <h2 className="mb-1 font-medium">{t("onboarding.recoveryTitle")}</h2>
                <p className="mb-3 text-sm text-muted-foreground">{t("onboarding.recoveryHint")}</p>
                <code className="mb-3 block break-all rounded-lg bg-muted p-3 text-center text-sm font-semibold tracking-wider">
                  {recKey.data}
                </code>
                <Checkbox className="mb-4" checked={savedKey} label={t("onboarding.recoverySaved")}
                  onCheckedChange={(v) => setSavedKey(v)} />
              </>
            ) : (
              <p className="mb-4 text-sm text-muted-foreground">{t("onboarding.ready")}</p>
            )}
            <div className="flex gap-2">
              <Button variant="outline" onClick={() => setStep("accounts")}>{t("onboarding.back")}</Button>
              <Button
                className="flex-1"
                disabled={!!recKey.data && !savedKey || finish.isPending}
                onClick={() => finish.mutate()}
              >
                {finish.isPending ? "…" : t("onboarding.finish")}
              </Button>
            </div>
            {finish.isError && <p className="mt-2 text-sm text-destructive">{String(finish.error)}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
