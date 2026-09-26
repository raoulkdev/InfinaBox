import { Fragment, type ReactNode } from "react";
import { ExternalLink } from "./ExternalLink";

// Renders the assistant's replies. The only markdown renderer already in the
// bundle is MDXEditor, which is a full editor — far too heavy to mount once
// per chat bubble. So this handles just the subset agent replies actually
// lean on: paragraphs, headings, bullet/numbered lists, fenced code,
// `inline code`, **bold**, *italic* and [links](https://...). Everything is
// built as React elements (never raw HTML), so nothing in a reply can inject
// markup; anything unrecognised simply shows as the text it is.

type Block =
  | { kind: "paragraph"; text: string }
  | { kind: "heading"; text: string }
  /** `start` is the first item's number, so an ordered list split up by
   * code blocks or paragraphs reads 1, 2, 3 — not 1, 1, 1. */
  | { kind: "list"; ordered: boolean; start: number; items: string[] }
  | { kind: "code"; text: string };

function parseBlocks(source: string): Block[] {
  const blocks: Block[] = [];
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  let paragraph: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length > 0) blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (/^\s*```/.test(line)) {
      flushParagraph();
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i++]);
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }

    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    if (heading) {
      flushParagraph();
      blocks.push({ kind: "heading", text: heading[1] });
      continue;
    }

    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    const numbered = /^\s*(\d+)[.)]\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      flushParagraph();
      const ordered = !bullet;
      const text = bullet ? bullet[1] : numbered![2];
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list" && last.ordered === ordered) last.items.push(text);
      else blocks.push({ kind: "list", ordered, start: numbered ? Number(numbered[1]) : 1, items: [text] });
      continue;
    }

    if (line.trim() === "") {
      flushParagraph();
      continue;
    }

    // A wrapped continuation of the previous list item.
    const last = blocks[blocks.length - 1];
    if (paragraph.length === 0 && last?.kind === "list" && /^\s+\S/.test(line)) {
      last.items[last.items.length - 1] += ` ${line.trim()}`;
      continue;
    }

    paragraph.push(line);
  }
  flushParagraph();
  return blocks;
}

// Italics need a non-word, non-`*` character (or line start) before the
// opening star, non-space characters just inside both stars, and no word
// character right after the closing one — so globs like "*.gd files and
// *.tscn" stay literal. Link text is capped and can't span lines, so a line
// full of `[` can't make matching quadratic.
const INLINE =
  /(`[^`]+`)|(\*\*[^*]+\*\*)|((?<![\w*])\*(?:[^*\s]|[^*\s][^*]*[^*\s])\*(?![\w*]))|(\[[^\]\n]{1,500}\]\(https?:\/\/[^)\s]+\))/g;

function renderInline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let lastIndex = 0;
  let n = 0;
  for (const match of text.matchAll(INLINE)) {
    const start = match.index ?? 0;
    if (start > lastIndex) out.push(text.slice(lastIndex, start));
    const token = match[0];
    const key = n++;
    if (match[1]) {
      out.push(
        <code key={key} className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (match[2]) {
      out.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (match[3]) {
      out.push(<em key={key}>{token.slice(1, -1)}</em>);
    } else {
      const link = /^\[([^\]]+)\]\((.+)\)$/.exec(token)!;
      out.push(
        <ExternalLink key={key} url={link[2]}>
          {link[1]}
        </ExternalLink>,
      );
    }
    lastIndex = start + token.length;
  }
  if (lastIndex < text.length) out.push(text.slice(lastIndex));
  return out;
}

function renderLines(text: string): ReactNode {
  // Single newlines inside a paragraph are kept, since agents often use
  // them deliberately (short step-by-step lines).
  return text.split("\n").map((line, i) => (
    <Fragment key={i}>
      {i > 0 && <br />}
      {renderInline(line)}
    </Fragment>
  ));
}

export function MessageText({ text }: { text: string }) {
  const blocks = parseBlocks(text);
  return (
    <div className="flex flex-col gap-2 text-sm leading-relaxed break-words">
      {blocks.map((block, i) => {
        switch (block.kind) {
          case "paragraph":
            return <p key={i}>{renderLines(block.text)}</p>;
          case "heading":
            return (
              <p key={i} className="font-medium">
                {renderInline(block.text)}
              </p>
            );
          case "list": {
            const List = block.ordered ? "ol" : "ul";
            return (
              <List key={i} start={block.ordered ? block.start : undefined} className={`flex flex-col gap-1 pl-5 ${block.ordered ? "list-decimal" : "list-disc"}`}>
                {block.items.map((item, j) => (
                  <li key={j}>{renderInline(item)}</li>
                ))}
              </List>
            );
          }
          case "code":
            return (
              <pre
                key={i}
                className="overflow-x-auto rounded-md border border-border bg-background p-2 font-mono text-xs leading-normal"
              >
                {block.text}
              </pre>
            );
        }
      })}
    </div>
  );
}
