import { Archive, Landmark, Nfc, Pencil, PiggyBank, Wallet } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { shade } from "@/lib/color";
import { formatMinor } from "@/lib/money";
import { cn } from "@/lib/utils";
import type { AccountRow, PaymentSystem } from "@/lib/types";

const KIND_ICONS = { card: Nfc, cash: Wallet, savings: PiggyBank, other: Landmark } as const;

function Chip() {
  return (
    <svg viewBox="0 0 48 36" className="h-7 w-9" aria-hidden>
      <rect width="48" height="36" rx="6" fill="#d8b545" stroke="#a8862a" strokeWidth="1.5" />
      <path d="M0 12h15M0 24h15M33 12h15M33 24h15M24 0v12M24 24v36" stroke="#a8862a" fill="none" strokeWidth="1.4" />
      <rect x="18" y="11" width="12" height="14" rx="2.5" fill="none" stroke="#a8862a" strokeWidth="1.4" />
    </svg>
  );
}

export function PsLogo({ ps }: { ps?: PaymentSystem | null }) {
  if (!ps) return null;
  switch (ps) {
    case "mastercard":
      return (
        <svg viewBox="0 0 36 22" className="h-6 w-9" aria-label="Mastercard">
          <circle cx="14" cy="11" r="9" fill="#eb001b" />
          <circle cx="22" cy="11" r="9" fill="#f79e1b" fillOpacity="0.85" />
        </svg>
      );
    case "visa":
      return <span className="font-black italic tracking-widest" style={{ fontSize: "1.1rem" }}>VISA</span>;
    case "mir":
      return <span className="text-sm font-extrabold tracking-wide">МИР</span>;
    case "belkart":
      return <span className="text-xs font-bold tracking-wide">Белкарт</span>;
    case "unionpay":
      return <span className="rounded-md bg-white/25 px-1.5 py-0.5 text-[10px] font-bold tracking-wide">UP</span>;
  }
}

interface AccountCardProps {
  a: Pick<AccountRow, "id" | "name" | "kind" | "color" | "currency" | "last4" | "bank" | "paymentSystem" | "balance" | "archived">;
  ownerName?: string;
  compact?: boolean;
  onEdit?: () => void;
  onArchive?: () => void;
}

export function AccountCard({ a, ownerName, compact, onEdit, onArchive }: AccountCardProps) {
  const { t } = useTranslation();
  const KindIcon = KIND_ICONS[a.kind] ?? Landmark;
  const isCard = a.kind === "card";
  const bg = `linear-gradient(135deg, ${shade(a.color, 18)} 0%, ${a.color} 45%, ${shade(a.color, -28)} 100%)`;

  return (
    <div
      className={cn(
        "group relative flex flex-col justify-between overflow-hidden rounded-2xl text-white shadow-[var(--card-shadow)] transition-transform duration-200 hover:-translate-y-0.5",
        compact ? "p-3" : "p-4",
        a.archived === 1 && "opacity-60 saturate-50",
      )}
      style={{ background: bg, aspectRatio: compact ? undefined : "1.586" }}
    >
      <div className="pointer-events-none absolute -right-12 -top-20 h-56 w-56 rounded-full bg-white/10 blur-2xl" />
      <div className="pointer-events-none absolute -bottom-24 -left-10 h-48 w-48 rounded-full bg-black/10 blur-2xl" />

      <div className="relative flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className={cn("truncate font-semibold uppercase tracking-wide opacity-95", compact ? "text-[10px]" : "text-xs")}>
            {a.bank || a.name}
          </div>
          {a.bank && (
            <div className={cn("truncate font-medium opacity-80", compact ? "text-[10px]" : "text-xs")}>{a.name}</div>
          )}
        </div>
        <KindIcon className={cn("shrink-0 opacity-85", compact ? "h-4 w-4" : "h-5 w-5")} />
      </div>

      {isCard && !compact && (
        <div className="relative mt-2 flex items-center gap-2">
          <Chip />
          <Nfc className="h-5 w-5 opacity-85" />
        </div>
      )}

      <div className="relative mt-auto flex items-end justify-between gap-2">
        <div className="min-w-0">
          {isCard && (
            <div className={cn("font-mono tracking-[0.2em] opacity-90", compact ? "text-[10px]" : "text-sm")}>
              •••• {a.last4 ?? "····"}
            </div>
          )}
          <div className={cn("font-display font-semibold leading-tight", compact ? "mt-0.5 text-base" : "mt-1 text-xl")}>
            {formatMinor(a.balance ?? 0, a.currency)}
          </div>
          <div className={cn("truncate opacity-75", compact ? "text-[9px]" : "text-[11px]")}>
            {ownerName ?? t("ui.accountKind." + a.kind)}
          </div>
        </div>
        <div className="shrink-0">
          <PsLogo ps={a.paymentSystem} />
        </div>
      </div>

      {(onEdit || onArchive) && (
        <div className="absolute right-2 top-2 flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          {onEdit && (
            <Button size="icon" variant="ghost" className="h-7 w-7 bg-black/15 text-white hover:bg-black/25 hover:text-white" title={t("ui.edit")} onClick={onEdit}>
              <Pencil className="h-3.5 w-3.5" />
            </Button>
          )}
          {onArchive && (
            <Button size="icon" variant="ghost" className="h-7 w-7 bg-black/15 text-white hover:bg-black/25 hover:text-white" title={t("ui.archive")} onClick={onArchive}>
              <Archive className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
