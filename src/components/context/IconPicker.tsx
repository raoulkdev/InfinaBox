import { useState } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

// A page's or folder's icon is an emoji. A plain grid of the ones a game
// project is likely to want, plus a box to paste any other.

const EMOJIS = [
  "📄", "📝", "📋", "📌", "📎", "📚", "📖", "🗒️", "🗂️", "📁", "🧭", "🗺️",
  "💡", "⭐", "🔥", "✨", "🎯", "🏆", "🎮", "🕹️", "👾", "🎲", "🧩", "🎨",
  "🖌️", "🖼️", "🎵", "🎧", "🔊", "🎬", "📷", "✏️", "⚔️", "🛡️", "🏹", "💣",
  "🚀", "🛸", "🌍", "🏰", "🏝️", "🌲", "🌙", "☀️", "❄️", "🌊", "⛰️", "🏙️",
  "🐉", "👻", "🤖", "🧙", "🧟", "🦊", "🐱", "🐸", "🍄", "🍎", "💎", "🔑",
  "💰", "⚙️", "🔧", "🧪", "🐛", "✅", "❓", "⚠️", "❤️", "💬", "👥", "🎁",
];

interface IconPickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  current: string | null;
  /** The chosen icon, or null to remove it. */
  onPick: (icon: string | null) => void;
}

export function IconPicker({ open, onOpenChange, current, onPick }: IconPickerProps) {
  const [custom, setCustom] = useState("");
  const pick = (icon: string | null) => {
    onPick(icon);
    onOpenChange(false);
    setCustom("");
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm" data-testid="icon-picker">
        <DialogHeader>
          <DialogTitle>Icon</DialogTitle>
        </DialogHeader>
        <div className="grid grid-cols-12 gap-0.5">
          {EMOJIS.map((e) => (
            <button
              key={e}
              type="button"
              data-testid="icon-choice"
              onClick={() => pick(e)}
              className={`flex size-7 items-center justify-center rounded-md text-lg hover:bg-accent ${current === e ? "bg-accent ring-1 ring-ring" : ""}`}
            >
              {e}
            </button>
          ))}
        </div>
        <form
          className="flex gap-2"
          onSubmit={(ev) => {
            ev.preventDefault();
            if (custom.trim()) pick(custom.trim());
          }}
        >
          <Input value={custom} onChange={(e) => setCustom(e.target.value)} placeholder="Or paste any emoji" maxLength={16} data-testid="icon-custom" className="h-8" />
          <Button type="submit" size="sm" disabled={!custom.trim()}>
            Use
          </Button>
        </form>
        {current && (
          <Button variant="ghost" size="sm" className="self-start" data-testid="icon-remove" onClick={() => pick(null)}>
            Remove icon
          </Button>
        )}
      </DialogContent>
    </Dialog>
  );
}
