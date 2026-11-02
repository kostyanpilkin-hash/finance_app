import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";
import { PlaceholderPage } from "@/pages/PlaceholderPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { PAGES, type PageId } from "./pages";

export default function App() {
  const { t } = useTranslation();
  const init = useSettingsStore((s) => s.init);
  const [page, setPage] = useState<PageId>("dashboard");

  useEffect(() => init(), [init]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const idx = Number(e.key) - 1;
      if (idx >= 0 && idx < 9) {
        e.preventDefault();
        setPage(PAGES[idx].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex h-screen text-foreground" data-page={page}>
      <aside className="flex w-56 shrink-0 flex-col border-r border-border/80 bg-[var(--sidebar-bg)] p-4 backdrop-blur">
        <h1 className="font-display mb-6 text-2xl font-semibold tracking-tight text-primary">{t("app.title")}</h1>
        <nav className="flex flex-col gap-1">
          {PAGES.map(({ id, icon: Icon }) => (
            <Button
              key={id}
              variant={page === id ? "default" : "ghost"}
              className="justify-start"
              onClick={() => setPage(id)}
            >
              <Icon className="h-4 w-4" />
              {t(`nav.${id}`)}
            </Button>
          ))}
        </nav>
      </aside>

      <main className="flex flex-1 flex-col overflow-auto p-6">
        {page === "settings" ? <SettingsPage /> : <PlaceholderPage page={page} />}
      </main>
    </div>
  );
}
