import { Check } from "lucide-react";
import { CATEGORY_COLORS } from "@/lib/categoryIcons";
import { cn } from "@/lib/utils";

interface ColorSwatchesProps {
  value?: string | null;
  onChange: (color: string) => void;
  colors?: readonly string[];
  className?: string;
}

/** Fixed palette picker — replaces the OS color input for categories/accounts/goals. */
export function ColorSwatches({ value, onChange, colors = CATEGORY_COLORS, className }: ColorSwatchesProps) {
  return (
    <div className={cn("flex flex-wrap gap-2", className)} role="radiogroup">
      {colors.map((c) => {
        const active = value?.toLowerCase() === c.toLowerCase();
        return (
          <button
            key={c}
            type="button"
            role="radio"
            aria-checked={active}
            aria-label={c}
            onClick={() => onChange(c)}
            style={{ background: c }}
            className={cn(
              "flex h-8 w-8 cursor-pointer items-center justify-center rounded-full text-white transition-transform",
              "hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
              active && "ring-2 ring-foreground ring-offset-2 ring-offset-background",
            )}
          >
            {active && <Check className="size-4" strokeWidth={3} />}
          </button>
        );
      })}
    </div>
  );
}
