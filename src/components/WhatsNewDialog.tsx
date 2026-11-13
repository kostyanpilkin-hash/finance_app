import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { WHATSNEW_SEEN_KEY, whatsNewFor } from "@/lib/whatsnew";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

/** What's-new dialog content for the currently installed version. */
export function WhatsNewDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const info = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const version = info.data?.version ?? "";
  const lang = (i18n.language?.slice(0, 2) || "ru") as "ru" | "be" | "en";
  const entry = whatsNewFor(version);
  const items = entry?.items[lang] ?? entry?.items.ru ?? [];

  return (
    <Dialog open={open} onClose={onClose} title={t("whatsnew.title", { version })} closeLabel={t("ui.close")}>
      {items.length > 0 ? (
        <ul className="mb-4 list-disc space-y-1.5 pl-5 text-sm">
          {items.map((x, i) => <li key={i}>{x}</li>)}
        </ul>
      ) : (
        <p className="mb-4 text-sm text-muted-foreground">{t("whatsnew.technical")}</p>
      )}
      <Button className="w-full" onClick={onClose}>{t("whatsnew.ok")}</Button>
    </Dialog>
  );
}

/** Opens the dialog once per version per device when the app was updated. */
export function WhatsNewAuto() {
  // `dismissed` covers the gap between closing and the settings refetch.
  const [dismissed, setDismissed] = useState(false);
  const info = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const queryClient = useQueryClient();

  const version = info.data?.version;
  // Missing key means an upgrade from a pre-feature build — fresh installs get
  // the key written during onboarding.
  const open = !!version && !!settings.data && settings.data[WHATSNEW_SEEN_KEY] !== version && !dismissed;

  const close = () => {
    setDismissed(true);
    if (version) {
      void api.setSetting(WHATSNEW_SEEN_KEY, version)
        .then(() => queryClient.invalidateQueries({ queryKey: ["settings"] }));
    }
  };
  return <WhatsNewDialog open={open} onClose={close} />;
}
