import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { LANGUAGES } from "@/i18n";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";

type Step = "language" | "users" | "accounts" | "key";

export function Onboarding() {
  const { t } = useTranslation();
  const { language, setLanguage } = useSettingsStore();
  const queryClient = useQueryClient();
  const [step, setStep] = useState<Step>("language");
  const [names, setNames] = useState<string[]>([""]);
  const [current, setCurrent] = useState(0);
  const [starter, setStarter] = useState<string[]>(["cash"]);
  const [savedKey, setSavedKey] = useState(false);

  const recKey = useQuery({ queryKey: ["recovery-key"], queryFn: api.recoveryKey });

  const finish = useMutation({
    mutationFn: () =>
      api.completeOnboarding({
        userNames: names.filter((n) => n.trim()),
        currentUser: Math.min(current, names.filter((n) => n.trim()).length - 1),
        starterAccounts: starter,
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["onboarding"] }),
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
            <Button className="w-full" onClick={() => setStep("users")}>{t("onboarding.next")}</Button>
          </div>
        )}

        {step === "users" && (
          <div>
            <h2 className="mb-1 font-medium">{t("onboarding.usersTitle")}</h2>
            <p className="mb-3 text-sm text-muted-foreground">{t("onboarding.usersHint")}</p>
            {names.map((n, i) => (
              <div key={i} className="mb-2 flex items-center gap-2">
                <input
                  className="flex-1 rounded-[var(--input-radius)] border border-border bg-background px-3 py-2 text-sm"
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
              <Button variant="outline" onClick={() => setStep("language")}>{t("onboarding.back")}</Button>
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
              <label key={k} className="mb-2 flex cursor-pointer items-center gap-3 rounded-lg border border-border p-3">
                <input type="checkbox" checked={starter.includes(k)} onChange={() => toggle(k)} />
                {t(`onboarding.account.${k}`)}
              </label>
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
                <label className="mb-4 flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={savedKey} onChange={(e) => setSavedKey(e.target.checked)} />
                  {t("onboarding.recoverySaved")}
                </label>
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
