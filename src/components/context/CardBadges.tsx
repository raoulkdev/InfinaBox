import type { CardType } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { badgeStyle, statusClasses, statusLabel, typeInfo } from "./cardTypes";

/** A card type as a small coloured pill. */
export function TypeBadge({ type, className }: { type: CardType | null | undefined; className?: string }) {
  const info = typeInfo(type);
  return (
    <span
      className={cn("inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[11px] font-medium", className)}
      style={badgeStyle(info.color)}
    >
      {info.label}
    </span>
  );
}

/** A card's status as a small pill; nothing when it has none. */
export function StatusPill({ status, className }: { status: string | null | undefined; className?: string }) {
  if (!status) return null;
  return (
    <span
      className={cn(
        "inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[11px] font-medium",
        statusClasses(status),
        className,
      )}
    >
      {statusLabel(status)}
    </span>
  );
}
