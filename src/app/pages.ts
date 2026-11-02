import {
  ArrowLeftRight,
  BarChart3,
  Bell,
  CreditCard,
  FileText,
  History,
  LayoutDashboard,
  Settings,
  Tags,
  Target,
  type LucideIcon,
} from "lucide-react";

export const PAGES = [
  { id: "dashboard", icon: LayoutDashboard },
  { id: "transactions", icon: ArrowLeftRight },
  { id: "accounts", icon: CreditCard },
  { id: "categories", icon: Tags },
  { id: "analytics", icon: BarChart3 },
  { id: "reports", icon: FileText },
  { id: "goals", icon: Target },
  { id: "reminders", icon: Bell },
  { id: "journal", icon: History },
  { id: "settings", icon: Settings },
] as const satisfies readonly { id: string; icon: LucideIcon }[];

export type PageId = (typeof PAGES)[number]["id"];
