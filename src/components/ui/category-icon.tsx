import { ICON_COMPONENTS } from "@/lib/categoryIcons";
import { cn } from "@/lib/utils";

/** Round coloured badge with a white pictogram; categories without an icon show a plain circle. */
export function CategoryIcon({ icon, color, size = 32, className }: {
  icon?: string | null; color?: string | null; size?: number; className?: string;
}) {
  const Icon = icon ? ICON_COMPONENTS[icon] : undefined;
  return (
    <span
      className={cn("inline-flex shrink-0 items-center justify-center rounded-full text-white", className)}
      style={{ width: size, height: size, background: color ?? "#6b7280" }}
    >
      {Icon && <Icon style={{ width: size * 0.55, height: size * 0.55 }} strokeWidth={2.2} />}
    </span>
  );
}
