import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function PinLock({ onUnlock }: { onUnlock: () => void }) {
  const { t } = useTranslation();
  const [pin, setPin] = useState("");
  const verify = useMutation({
    mutationFn: () => api.pinVerify(pin),
    onSuccess: (ok) => {
      if (ok) onUnlock();
      else setPin("");
    },
  });
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 bg-background">
      <div className="text-lg font-medium">{t("ui.locked")}</div>
      <Input
        type="password"
        inputMode="numeric"
        maxLength={8}
        autoFocus
        className="w-40 text-center text-2xl tracking-[0.5em]"
        value={pin}
        onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))}
        onKeyDown={(e) => e.key === "Enter" && pin && verify.mutate()}
      />
      {verify.isError && <p className="text-sm text-destructive">{String(verify.error)}</p>}
      {verify.isSuccess && !verify.data && <p className="text-sm text-destructive">{t("ui.wrongPin")}</p>}
      <Button onClick={() => verify.mutate()} disabled={!pin || verify.isPending}>{t("ui.unlock")}</Button>
    </div>
  );
}
