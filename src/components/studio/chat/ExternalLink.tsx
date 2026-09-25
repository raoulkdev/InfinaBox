import { useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

interface ExternalLinkProps {
  url: string;
  children: ReactNode;
  className?: string;
}

// Opens `url` in the user's browser. If the OS refuses (no default browser,
// opener permission missing), the URL is shown inline as selectable text
// instead, so the user can still copy it rather than clicking a dead link.
export function ExternalLink({ url, children, className }: ExternalLinkProps) {
  const [failed, setFailed] = useState(false);

  return (
    <>
      <button
        type="button"
        onClick={() => openUrl(url).catch(() => setFailed(true))}
        className={className ?? "text-foreground underline underline-offset-3 hover:text-foreground/80"}
        title={url}
      >
        {children}
      </button>
      {failed && (
        <span className="font-mono text-xs break-all text-muted-foreground select-all">
          {" "}
          (couldn't open your browser — copy this link: {url})
        </span>
      )}
    </>
  );
}
