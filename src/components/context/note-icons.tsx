import {
  Apple, Bird, Bookmark, BookOpen, Bot, Box, Brush, Bug, Building2, Calendar, Camera, Castle, Cat, CheckCircle2, CircleHelp, Clock, Cloud, Cog, Coins, Compass, Crown, Dice5, Dog,
  FileText, Film, Fish, Flag, Flame, Flower2, Folder, Gamepad2, Gem, Ghost, Gift, Globe, Hammer, Headphones, Heart, Image, Key, Layers, Leaf, Lightbulb, Lock, Map, MessageCircle,
  Mic, Moon, Mountain, Music, Notebook, Palette, Pin, Puzzle, Rocket, Shield, Skull, Snowflake, Sparkles, Star, Sun, Sword, Target, TreePine, Trophy, TriangleAlert, User, Users,
  Volume2, Waves, Wrench, Zap, type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";

// Icons for pages and folders come in two kinds, stored in the same `icon`
// value: an emoji ("🗺️"), or a plain black-and-white line icon written as
// "lucide:<name>" that takes the colour of the text around it.

export const MONO_PREFIX = "lucide:";

export const NOTE_ICONS: Record<string, LucideIcon> = {
  "file-text": FileText, folder: Folder, notebook: Notebook, "book-open": BookOpen, bookmark: Bookmark, pin: Pin, flag: Flag, calendar: Calendar, clock: Clock, layers: Layers,
  lightbulb: Lightbulb, star: Star, heart: Heart, flame: Flame, zap: Zap, sparkles: Sparkles, target: Target, trophy: Trophy, crown: Crown, gem: Gem,
  "gamepad-2": Gamepad2, "dice-5": Dice5, puzzle: Puzzle, sword: Sword, shield: Shield, skull: Skull, ghost: Ghost, bot: Bot, bug: Bug, rocket: Rocket,
  map: Map, compass: Compass, globe: Globe, mountain: Mountain, "tree-pine": TreePine, sun: Sun, moon: Moon, cloud: Cloud, snowflake: Snowflake, waves: Waves,
  castle: Castle, "building-2": Building2, box: Box, key: Key, lock: Lock, coins: Coins, gift: Gift, hammer: Hammer, wrench: Wrench, cog: Cog,
  music: Music, headphones: Headphones, "volume-2": Volume2, mic: Mic, image: Image, palette: Palette, brush: Brush, camera: Camera, film: Film, "message-circle": MessageCircle,
  user: User, users: Users, cat: Cat, dog: Dog, bird: Bird, fish: Fish, apple: Apple, leaf: Leaf, "flower-2": Flower2, "circle-help": CircleHelp,
  "check-circle-2": CheckCircle2, "triangle-alert": TriangleAlert,
};

export const isMono = (icon: string | null | undefined): icon is string => !!icon && icon.startsWith(MONO_PREFIX);

/** An icon as it should be drawn: a line icon in the text colour, or the emoji itself. */
export function NoteIcon({ value, className }: { value: string; className?: string }) {
  if (isMono(value)) {
    const Icon = NOTE_ICONS[value.slice(MONO_PREFIX.length)];
    if (Icon) return <Icon className={cn("shrink-0", className)} aria-hidden />;
  }
  return (
    <span className={cn("shrink-0 text-center leading-none", className)} aria-hidden>
      {isMono(value) ? "" : value}
    </span>
  );
}
