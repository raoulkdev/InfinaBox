import { useState } from "react";
import { AlertCircle, Check, ChevronDown, Loader2, Plus } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import type { ThreadSummary } from "@/lib/studio-types";

interface ThreadPickerProps {
  threads: ThreadSummary[];
  activeThreadId: string | null;
  /** Threads with a turn still running in the background. */
  runningThreadIds: ReadonlySet<string>;
  onSelect: (threadId: string) => void;
  /** Creates the thread for real; rejects with the backend's error text. */
  onCreate: (title: string) => Promise<void>;
  disabled: boolean;
}

// The chat header's thread switcher: separate conversations per topic
// ("Boss fight", "Main menu"), each saved as its own file in the project.
export function ThreadPicker({
  threads,
  activeThreadId,
  runningThreadIds,
  onSelect,
  onCreate,
  disabled,
}: ThreadPickerProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const active = threads.find((t) => t.id === activeThreadId);
  // Newest first, matching how "latest thread" is picked on open.
  const sorted = [...threads].sort((a, b) => b.created_at - a.created_at);

  function handleOpenChange(open: boolean) {
    setDialogOpen(open);
    if (!open) {
      setTitle("");
      setCreateError(null);
    }
  }

  async function handleCreate() {
    const trimmed = title.trim();
    if (!trimmed || creating) return;
    setCreating(true);
    setCreateError(null);
    try {
      await onCreate(trimmed);
      handleOpenChange(false);
    } catch (err) {
      setCreateError(String(err));
    } finally {
      setCreating(false);
    }
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild disabled={disabled}>
          <Button type="button" size="xs" variant="ghost" className="max-w-56 gap-1 text-muted-foreground">
            <span className="truncate">{active?.title ?? "Conversations"}</span>
            <ChevronDown />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuLabel>Conversations</DropdownMenuLabel>
          {sorted.map((thread) => (
            <DropdownMenuItem key={thread.id} onSelect={() => onSelect(thread.id)}>
              <span className="min-w-0 flex-1 truncate">{thread.title}</span>
              {runningThreadIds.has(thread.id) && (
                <Loader2 className="animate-spin text-muted-foreground" aria-label="AI is working" />
              )}
              {thread.id === activeThreadId && <Check />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setDialogOpen(true)}>
            <Plus />
            New conversation
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={dialogOpen} onOpenChange={handleOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New conversation</DialogTitle>
            <DialogDescription>
              Start a fresh conversation about one part of your game. Your other conversations stay as they are.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Input
              autoFocus
              value={title}
              placeholder="e.g. Boss fight"
              onChange={(e) => {
                setTitle(e.target.value);
                setCreateError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCreate();
              }}
            />
            {createError && (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertDescription>{createError}</AlertDescription>
              </Alert>
            )}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => handleOpenChange(false)}>
              Cancel
            </Button>
            <Button type="button" disabled={!title.trim() || creating} onClick={() => void handleCreate()}>
              {creating ? "Creating…" : "Create"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
