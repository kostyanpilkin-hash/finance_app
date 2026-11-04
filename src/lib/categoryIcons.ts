import type { LucideIcon } from "lucide-react";
import {
  Baby, Banknote, Beer, Bike, BookOpen, Briefcase, Bus, Camera, Car, Cat, CircleHelp, Coffee, Coins,
  CreditCard, Dog, Droplet, Dumbbell, Film, Flame, Flower2, Fuel, Gamepad2, Gift, Glasses, GraduationCap,
  HandCoins, HeartPulse, House, Lamp, Landmark, Laptop, Music, Palette, Percent, PiggyBank, Pill, Pizza,
  Plane, Popcorn, Receipt, Repeat, Scissors, Shield, Shirt, ShoppingBag, ShoppingCart, Smartphone, Sofa,
  Sparkles, Stethoscope, Store, Ticket, TrainFront, TreePine, TrendingUp, Tv, Umbrella, Utensils, Wallet,
  Wifi, Wine, Wrench, Zap,
} from "lucide-react";
import catalogue from "./category-icons.json";

/** Lucide component per icon id; ids and bot emoji live in category-icons.json. */
export const ICON_COMPONENTS: Record<string, LucideIcon> = {
  "shopping-cart": ShoppingCart, "shopping-bag": ShoppingBag, store: Store, utensils: Utensils,
  coffee: Coffee, pizza: Pizza, wine: Wine, beer: Beer, bus: Bus, "train-front": TrainFront, car: Car,
  fuel: Fuel, bike: Bike, plane: Plane, house: House, sofa: Sofa, lamp: Lamp, zap: Zap, droplet: Droplet,
  flame: Flame, wrench: Wrench, wifi: Wifi, smartphone: Smartphone, laptop: Laptop, tv: Tv,
  "heart-pulse": HeartPulse, pill: Pill, stethoscope: Stethoscope, dumbbell: Dumbbell, sparkles: Sparkles,
  scissors: Scissors, shirt: Shirt, glasses: Glasses, baby: Baby, "graduation-cap": GraduationCap,
  "book-open": BookOpen, popcorn: Popcorn, film: Film, music: Music, "gamepad-2": Gamepad2, ticket: Ticket,
  palette: Palette, camera: Camera, umbrella: Umbrella, "tree-pine": TreePine, "flower-2": Flower2,
  dog: Dog, cat: Cat, gift: Gift, repeat: Repeat, receipt: Receipt, shield: Shield, landmark: Landmark,
  "credit-card": CreditCard, wallet: Wallet, banknote: Banknote, coins: Coins, "hand-coins": HandCoins,
  "piggy-bank": PiggyBank, briefcase: Briefcase, percent: Percent, "trending-up": TrendingUp,
  "circle-help": CircleHelp,
};

export const CATEGORY_ICONS: { id: string; emoji: string }[] = catalogue;

/** Icon id stored for categories without a pictogram. */
export const NO_ICON = "tag";

/** Monefy-like palette for category circles. */
export const CATEGORY_COLORS = [
  "#16a34a", "#059669", "#0d9488", "#0891b2", "#2563eb", "#4f46e5", "#7c3aed", "#9333ea",
  "#c026d3", "#db2777", "#e11d48", "#dc2626", "#ea580c", "#f59e0b", "#a16207", "#4b5563",
];
