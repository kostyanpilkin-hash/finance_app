import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { cn } from "@/lib/utils";
import { todayIso } from "@/lib/money";

function monthEnd(month: string) {
  const [y, m] = month.split("-").map(Number);
  return `${month}-${String(new Date(y, m, 0).getDate()).padStart(2, "0")}`;
}

/** Month grid popover; days with transactions get a dot. onPick(day) selects. */
export function DayCalendar({ selected, onPick }: { selected: string; onPick: (day: string) => void }) {
  const { t, i18n } = useTranslation();
  const [month, setMonth] = useState(selected.slice(0, 7));
  const days = useQuery({
    queryKey: qk.txDays(month),
    queryFn: () => api.transactionDays(`${month}-01`, monthEnd(month)),
  });
  const marked = useMemo(() => new Set(days.data ?? []), [days.data]);
  const today = todayIso();

  const [y, m] = month.split("-").map(Number);
  const daysInMonth = new Date(y, m, 0).getDate();
  const firstWd = (new Date(y, m - 1, 1).getDay() + 6) % 7; // 0 = Monday
  const cells: (string | null)[] = Array.from({ length: firstWd }, () => null);
  for (let d = 1; d <= daysInMonth; d++) cells.push(`${month}-${String(d).padStart(2, "0")}`);

  const title = new Date(y, m - 1, 1).toLocaleDateString(i18n.language, { month: "long", year: "numeric" });
  const shift = (n: number) => {
    const d = new Date(y, m - 1 + n, 1);
    setMonth(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`);
  };

  return (
    <div className="absolute left-1/2 top-full z-20 mt-1 w-72 -translate-x-1/2 rounded-2xl border border-border bg-card p-3 shadow-xl">
      <div className="mb-2 flex items-center justify-between">
        <button type="button" className="rounded-lg p-1.5 transition-colors hover:bg-muted" onClick={() => shift(-1)}>
          <ChevronLeft className="h-4 w-4" />
        </button>
        <div className="text-sm font-medium capitalize">{title}</div>
        <button type="button" className="rounded-lg p-1.5 transition-colors hover:bg-muted" onClick={() => shift(1)}>
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
      <div className="grid grid-cols-7 gap-0.5 text-center">
        {[1, 2, 3, 4, 5, 6, 7].map((d) => (
          <div key={d} className="py-1 text-xs font-semibold uppercase text-muted-foreground">{t(`ui.wd.${d}`)}</div>
        ))}
        {cells.map((d, i) =>
          d === null ? (
            <div key={`e${i}`} />
          ) : (
            <button
              key={d}
              type="button"
              onClick={() => onPick(d)}
              className={cn(
                "relative flex h-8 items-center justify-center rounded-lg text-sm transition-colors",
                d === selected
                  ? "bg-primary text-primary-foreground"
                  : d === today
                    ? "font-semibold text-primary hover:bg-muted"
                    : "hover:bg-muted",
              )}
            >
              {Number(d.slice(-2))}
              {marked.has(d) && (
                <span className={cn("absolute bottom-1 h-1 w-1 rounded-full", d === selected ? "bg-primary-foreground" : "bg-primary")} />
              )}
            </button>
          ),
        )}
      </div>
      <div className="mt-2 flex justify-center">
        <button type="button" className="rounded-lg px-3 py-1 text-xs font-medium text-primary transition-colors hover:bg-muted"
          onClick={() => onPick(today)}>
          {t("ui.today")}
        </button>
      </div>
    </div>
  );
}
