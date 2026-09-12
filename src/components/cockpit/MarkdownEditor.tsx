import { useEffect, useRef } from "react";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { markdown } from "@codemirror/lang-markdown";
import { basicSetup } from "codemirror";

interface MarkdownEditorProps {
  /** Initial document text. Only read on mount — give this component a
   * `key` (e.g. the file path) to force a remount when switching files. */
  initialValue: string;
  onChange: (value: string) => void;
  /** Called for Cmd+S / Ctrl+S while the editor has focus. */
  onSave: () => void;
}

/** Wraps a CodeMirror 6 EditorView as an uncontrolled component. CodeMirror
 * owns the DOM and its own document state; we only listen for changes and
 * push text out via `onChange`, rather than fighting it for control on
 * every keystroke. */
export function MarkdownEditor({
  initialValue,
  onChange,
  onSave,
}: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Refs so the keymap/updateListener closures always call the latest
  // callback without needing to tear down and rebuild the EditorView.
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  useEffect(() => {
    const state = EditorState.create({
      doc: initialValue,
      extensions: [
        basicSetup,
        markdown(),
        EditorView.lineWrapping,
        keymap.of([
          {
            key: "Mod-s",
            run: () => {
              onSaveRef.current();
              return true;
            },
            preventDefault: true,
          },
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { overflow: "auto", fontFamily: "var(--font-sans)" },
          "&.cm-focused": { outline: "none" },
        }),
      ],
    });

    const view = new EditorView({
      state,
      parent: hostRef.current!,
    });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Intentionally only re-run when the host element identity changes
    // (i.e. never, for a mounted component) — callers remount this
    // component with a `key` when the underlying file changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={hostRef} className="h-full min-h-0 w-full" />;
}
