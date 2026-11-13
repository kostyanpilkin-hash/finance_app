import * as React from "react";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

export interface RichSelectItem {
  value: string;
  label: React.ReactNode;
  /** Optional leading visual — icon node or a `dot` color. */
  icon?: React.ReactNode;
  dot?: string | null;
  disabled?: boolean;
}

export interface RichSelectProps {
  value: string;
  onChange: (value: string) => void;
  items: RichSelectItem[];
  placeholder?: React.ReactNode;
  className?: string;
  invalid?: boolean;
  disabled?: boolean;
  id?: string;
  "aria-label"?: string;
}

/** Styled dropdown: a button that opens a card with rich items (icons, dots),
 *  Escape/backdrop close, arrow-key navigation. Same 40px field geometry as Input. */
export function RichSelect({
  value, onChange, items, placeholder, className, invalid, disabled, id, "aria-label": ariaLabel,
}: RichSelectProps) {
  const [open, setOpen] = React.useState(false);
  const [active, setActive] = React.useState(-1);
  const rootRef = React.useRef<HTMLDivElement>(null);
  const listRef = React.useRef<HTMLDivElement>(null);

  const selected = items.find((i) => i.value === value);
  const enabledCount = React.useMemo(() => items.filter((i) => !i.disabled).length, [items]);

  const close = () => { setOpen(false); setActive(-1); };
  const toggle = () => {
    if (disabled) return;
    setOpen((v) => {
      const next = !v;
      if (next) {
        const idx = items.findIndex((i) => i.value === value && !i.disabled);
        setActive(idx >= 0 ? idx : items.findIndex((i) => !i.disabled));
      }
      return next;
    });
  };

  const move = (dir: 1 | -1) => {
    if (enabledCount === 0) return;
    let i = active;
    for (let n = 0; n < items.length; n++) {
      i = (i + dir + items.length) % items.length;
      if (!items[i].disabled) break;
    }
    setActive(i);
    listRef.current?.children[i]?.scrollIntoView({ block: "nearest" });
  };

  const pick = (v: string) => { onChange(v); close(); };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { e.stopPropagation(); close(); return; }
    if (!open && (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ")) {
      e.preventDefault(); toggle(); return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if ((e.key === "Enter" || e.key === " ") && active >= 0 && items[active]) {
      e.preventDefault(); pick(items[active].value);
    } else if (e.key === "Tab") { close(); }
  };

  const lead = (i?: RichSelectItem) =>
    i?.icon ? (
      <span className="flex h-5 w-5 shrink-0 items-center justify-center">{i.icon}</span>
    ) : i?.dot ? (
      <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: i.dot }} />
    ) : null;

  return (
    <div ref={rootRef} className={cn("relative", className)} onKeyDown={onKey}>
      <button
        type="button"
        id={id}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-invalid={invalid || undefined}
        disabled={disabled}
        onClick={toggle}
        className={cn(
          "flex h-10 w-full cursor-pointer items-center gap-2 rounded-[var(--input-radius)] border bg-card px-3 text-left text-sm text-foreground",
          "border-field-border transition-colors hover:border-ring/50",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
          "disabled:cursor-not-allowed disabled:opacity-50",
          invalid && "border-destructive focus-visible:ring-destructive",
          open && "border-ring/60",
        )}
      >
        {lead(selected)}
        <span className={cn("min-w-0 flex-1 truncate", !selected && "text-muted-foreground")}>
          {selected ? selected.label : placeholder ?? "…"}
        </span>
        <ChevronDown className={cn("h-4 w-4 shrink-0 text-muted-foreground transition-transform", open && "rotate-180")} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-30" onClick={close} />
          <div
            ref={listRef}
            role="listbox"
            aria-activedescendant={active >= 0 ? `rs-${id ?? "x"}-${active}` : undefined}
            className="animate-fade-up absolute left-0 right-0 top-full z-40 mt-1 max-h-64 overflow-auto rounded-xl border border-border bg-card p-1 shadow-[var(--card-shadow)]"
          >
            {items.length === 0 && (
              <div className="px-3 py-2 text-sm text-muted-foreground">—</div>
            )}
            {items.map((i, idx) => (
              <button
                key={i.value || `__empty_${idx}`}
                id={`rs-${id ?? "x"}-${idx}`}
                type="button"
                role="option"
                aria-selected={i.value === value}
                disabled={i.disabled}
                onMouseEnter={() => setActive(idx)}
                onClick={() => pick(i.value)}
                className={cn(
                  "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-2 text-left text-sm transition-colors",
                  i.value === value ? "bg-primary/10 font-medium text-foreground" : "text-foreground",
                  idx === active && "bg-muted",
                  i.disabled && "cursor-not-allowed opacity-50",
                )}
              >
                {lead(i)}
                <span className="min-w-0 flex-1 truncate">{i.label}</span>
                {i.value === value && <Check className="h-4 w-4 shrink-0 text-primary" />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
