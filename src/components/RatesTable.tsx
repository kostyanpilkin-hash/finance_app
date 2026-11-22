import { useTranslation } from "react-i18next";
import { formatRate, rateUnits } from "@/lib/fx";
import type { RateOverview } from "@/lib/types";

/** One NBRB row from rates_latest — used to merge in currencies BSB lacks. */
export interface NbrbRow {
  currency: string;
  rateBynScaled: number;
  scale: number;
  date: string;
}

/**
 * BSB app / BSB card / NBRB rates side by side. Shared between
 * Settings → Rates (all currencies) and the optional dashboard strip
 * (USD/EUR/RUB).
 */
export function RatesTable({
  rows,
  nbrbDate,
}: {
  rows: RateOverview[];
  nbrbDate?: string;
}) {
  const { t, i18n } = useTranslation();
  const fmt = (v: number | null | undefined, cur: string) =>
    v ? formatRate(v, cur, i18n.language) : "—";
  const pair = (p: RateOverview["bsbApp"], cur: string) =>
    p ? `${fmt(p.buy, cur)} / ${fmt(p.sell, cur)}` : "—";
  return (
    <table className="w-full text-left tabular-nums">
      <thead className="text-xs text-muted-foreground">
        <tr>
          <th className="py-1 pr-4 font-medium" />
          <th className="py-1 pr-4 font-medium">
            {t("ui.fx.bsbApp")} <span className="font-normal">({t("ui.fx.buy")} / {t("ui.fx.sell")})</span>
          </th>
          <th className="py-1 pr-4 font-medium">
            {t("ui.fx.bsbCard")} <span className="font-normal">({t("ui.fx.buy")} / {t("ui.fx.sell")})</span>
          </th>
          <th className="py-1 font-medium">
            {t("ui.fx.nbrb")}
            {nbrbDate && <span className="font-normal"> · {nbrbDate}</span>}
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.currency} className="border-t border-border/50">
            <td className="py-1.5 pr-4 font-medium">
              {rateUnits(r.currency) > 1 ? `${rateUnits(r.currency)} ` : ""}
              {r.currency}
            </td>
            <td className="py-1.5 pr-4">{pair(r.bsbApp, r.currency)}</td>
            <td className="py-1.5 pr-4">{pair(r.bsbCard, r.currency)}</td>
            <td className="py-1.5">{fmt(r.nbrb, r.currency)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
