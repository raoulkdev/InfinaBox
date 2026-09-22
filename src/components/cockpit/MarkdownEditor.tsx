import {
  BlockTypeSelect,
  BoldItalicUnderlineToggles,
  CodeToggle,
  codeBlockPlugin,
  codeMirrorPlugin,
  CreateLink,
  headingsPlugin,
  InsertCodeBlock,
  InsertTable,
  InsertThematicBreak,
  linkDialogPlugin,
  linkPlugin,
  listsPlugin,
  ListsToggle,
  markdownShortcutPlugin,
  MDXEditor,
  quotePlugin,
  Separator,
  tablePlugin,
  thematicBreakPlugin,
  toolbarPlugin,
  UndoRedo,
} from "@mdxeditor/editor";
import { basicDark } from "cm6-theme-basic-dark";
import "@mdxeditor/editor/style.css";
import "@/components/cockpit/markdown-editor-dark.css";

interface MarkdownEditorProps {
  /** Initial document text. Only read on mount — give this component a
   * `key` (e.g. the file path) to force a remount when switching files.
   * MDXEditor treats its own `markdown` prop the same way (read once, on
   * mount), so this uncontrolled/remount-on-key contract carries over
   * unchanged from the CodeMirror implementation. */
  initialValue: string;
  onChange: (value: string) => void;
  /** Called for Cmd+S / Ctrl+S while the editor has focus. */
  onSave: () => void;
}

function toolbarContents() {
  return (
    <>
      <UndoRedo />
      <Separator />
      <BlockTypeSelect />
      <Separator />
      <BoldItalicUnderlineToggles />
      <CodeToggle />
      <Separator />
      <ListsToggle />
      <Separator />
      <CreateLink />
      <InsertTable />
      <InsertThematicBreak />
      <InsertCodeBlock />
    </>
  );
}

/** WYSIWYG markdown editor backed by MDXEditor. Renders real headings,
 * lists, bold/italic, links, blockquotes, code blocks, and tables inline —
 * replacing the old CodeMirror "live preview" hide/reveal trick with an
 * actual rich-text surface, while still round-tripping to a plain markdown
 * string via `onChange`. */
export function MarkdownEditor({
  initialValue,
  onChange,
  onSave,
}: MarkdownEditorProps) {
  return (
    <div
      className="h-full min-h-0 w-full overflow-auto p-2"
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSave();
        }
      }}
    >
      <MDXEditor
        markdown={initialValue}
        onChange={(markdown, initialMarkdownNormalize) => {
          // MDXEditor fires onChange once up front to report the markdown
          // it normalized the initial value into (bullet style, spacing,
          // etc.) — not a real edit. Ignoring it keeps a freshly opened
          // file from immediately showing as "unsaved".
          if (initialMarkdownNormalize) return;
          onChange(markdown);
        }}
        className="dark-theme dark-editor"
        contentEditableClassName="mdx-editor-content"
        plugins={[
          headingsPlugin(),
          listsPlugin(),
          quotePlugin(),
          thematicBreakPlugin(),
          linkPlugin(),
          linkDialogPlugin(),
          tablePlugin(),
          codeBlockPlugin({ defaultCodeBlockLanguage: "" }),
          codeMirrorPlugin({
            codeBlockLanguages: { "": "Plain text", js: "JavaScript", ts: "TypeScript", tsx: "TSX", jsx: "JSX", json: "JSON", bash: "Bash", sh: "Shell", css: "CSS", html: "HTML", rust: "Rust", python: "Python", sql: "SQL", yaml: "YAML", md: "Markdown" },
            codeMirrorExtensions: [basicDark],
          }),
          markdownShortcutPlugin(),
          toolbarPlugin({ toolbarContents }),
        ]}
      />
    </div>
  );
}
