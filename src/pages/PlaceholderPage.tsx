import { useTranslation } from "react-i18next";
import type { PageId } from "@/app/pages";

export function PlaceholderPage({ page }: { page: Exclude<PageId, "settings"> }) {
  const { t } = useTranslation();
  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl">
      <h2 className="font-display mb-2 text-2xl font-semibold tracking-tight">{t(`nav.${page}`)}</h2>
      <p className="mb-6 text-muted-foreground">{t(`pages.${page}`)}</p>
      <div className="rounded-2xl border border-dashed border-border bg-card/70 p-10 text-center text-sm text-muted-foreground">
        {t("pages.comingSoon")}
      </div>
    </div>
  );
}
