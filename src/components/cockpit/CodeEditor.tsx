import { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import type { Extension } from "@uiw/react-codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";

interface CodeEditorProps {
  /** Used only to pick a syntax-highlighting language by extension — same
   * uncontrolled/remount-on-key contract as MarkdownEditor/GraphEditor
   * otherwise: `initialValue` is read once on mount, give this a
   * `key={path}` to force a remount when switching files. */
  path: string;
  initialValue: string;
  onChange: (value: string) => void;
  /** Called for Cmd+S / Ctrl+S while the editor has focus. */
  onSave: () => void;
}

// A handful of the text formats a game-dev project's own source is actually
// likely to contain — this is highlighting only, never a hard gate on what
// can be opened: any other extension still opens as plain, uncolored text
// rather than being refused.
function languageExtension(path: string): Extension[] {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
    case "mjs":
    case "cjs":
      return [javascript({ jsx: true, typescript: ext.startsWith("ts") })];
    case "json":
      return [json()];
    case "css":
      return [css()];
    case "html":
      return [html()];
    case "md":
    case "markdown":
      return [markdown()];
    case "py":
    // GDScript (Godot's scripting language) has no CodeMirror language
    // package of its own — its syntax is deliberately Python-like enough
    // (indentation-based blocks, `func`/`if`/`for` keywords, `#` comments)
    // that Python's highlighter reads as genuinely useful here, not just
    // a fallback.
    case "gd":
      return [python()];
    case "rs":
      return [rust()];
    default:
      return [];
  }
}

/** A CodeMirror-backed text editor for the Build tab's "mini VS Code"
 * window — general-purpose source editing for anything text-based in the
 * project (not just the docs formats MarkdownEditor/GraphEditor handle),
 * round-tripped via `onChange`/`onSave` the same way those two are. */
export function CodeEditor({ path, initialValue, onChange, onSave }: CodeEditorProps) {
  const extensions = useMemo(() => languageExtension(path), [path]);

  return (
    <div
      className="h-full min-h-0 w-full"
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSave();
        }
      }}
    >
      <CodeMirror
        value={initialValue}
        onChange={onChange}
        extensions={extensions}
        theme={oneDark}
        height="100%"
        className="h-full text-sm"
        basicSetup={{ foldGutter: true, highlightActiveLine: true }}
      />
    </div>
  );
}
