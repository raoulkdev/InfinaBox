import type { ReactNode } from "react";

interface MarkdownPreviewProps {
  content: string;
}

interface ParsedDoc {
  frontmatter: [string, string][] | null;
  body: string;
}

/** Splits off a leading `---\n...\n---` YAML frontmatter block, if present,
 * without attempting real YAML parsing (`key: value` lines only — good
 * enough for the flat frontmatter GDDs actually use). */
function splitFrontmatter(content: string): ParsedDoc {
  if (!content.startsWith("---")) return { frontmatter: null, body: content };
  const end = content.indexOf("\n---", 3);
  if (end === -1) return { frontmatter: null, body: content };

  const raw = content.slice(3, end).trim();
  const body = content.slice(end + 4).replace(/^\n/, "");
  const frontmatter: [string, string][] = [];
  for (const line of raw.split("\n")) {
    const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (match) frontmatter.push([match[1], match[2]]);
  }
  return { frontmatter, body };
}

/** Renders bold and italic spans inline. Anything else, including
 * `[[wiki-links]]`, passes through as inert plain text on purpose —
 * resolving those into real links is a later milestone. */
function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /\*\*([^*]+)\*\*|__([^_]+)__|\*([^*]+)\*|_([^_]+)_/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let i = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    const bold = match[1] ?? match[2];
    const italic = match[3] ?? match[4];
    if (bold !== undefined) {
      parts.push(<strong key={`${keyPrefix}-${i++}`}>{bold}</strong>);
    } else if (italic !== undefined) {
      parts.push(<em key={`${keyPrefix}-${i++}`}>{italic}</em>);
    }
    lastIndex = pattern.lastIndex;
  }
  if (lastIndex < text.length) parts.push(text.slice(lastIndex));
  return parts;
}

function Heading({ level, children }: { level: number; children: ReactNode }) {
  const className =
    level === 1
      ? "text-xl font-semibold tracking-tight"
      : level === 2
        ? "mt-1 text-base font-semibold tracking-tight"
        : "text-sm font-medium tracking-tight";
  switch (Math.min(level, 3)) {
    case 1:
      return <h1 className={className}>{children}</h1>;
    case 2:
      return <h2 className={className}>{children}</h2>;
    default:
      return <h3 className={className}>{children}</h3>;
  }
}

/** Minimal, hand-rolled markdown renderer — headings, paragraphs, `-`/`*`
 * lists, bold/italic. Not CommonMark-complete on purpose; this is a
 * nice-to-have preview, not the source of truth (the editor is). */
function renderBody(body: string) {
  const lines = body.split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;
  let key = 0;

  const isHeading = (l: string) => /^(#{1,6})\s+/.test(l);
  const isListItem = (l: string) => /^\s*[-*]\s+/.test(l);

  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") {
      i++;
      continue;
    }

    const headingMatch = line.match(/^(#{1,6})\s+(.*)$/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      blocks.push(
        <Heading level={level} key={key}>
          {renderInline(headingMatch[2], `h${key}`)}
        </Heading>,
      );
      key++;
      i++;
      continue;
    }

    if (isListItem(line)) {
      const items: string[] = [];
      while (i < lines.length && isListItem(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*]\s+/, ""));
        i++;
      }
      blocks.push(
        <ul key={key} className="list-disc space-y-1 pl-5 text-sm">
          {items.map((item, idx) => (
            <li key={idx}>{renderInline(item, `li${key}-${idx}`)}</li>
          ))}
        </ul>,
      );
      key++;
      continue;
    }

    const paraLines: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !isHeading(lines[i]) && !isListItem(lines[i])) {
      paraLines.push(lines[i]);
      i++;
    }
    blocks.push(
      <p key={key} className="text-sm leading-relaxed text-foreground/90">
        {renderInline(paraLines.join(" "), `p${key}`)}
      </p>,
    );
    key++;
  }

  return blocks;
}

export function MarkdownPreview({ content }: MarkdownPreviewProps) {
  const { frontmatter, body } = splitFrontmatter(content);

  return (
    <div className="flex flex-col gap-3 p-4">
      {frontmatter && frontmatter.length > 0 && (
        <div className="flex flex-col gap-1 border border-border bg-background px-3 py-2 text-xs">
          {frontmatter.map(([k, v]) => (
            <div key={k} className="flex gap-2">
              <span className="text-muted-foreground">{k}:</span>
              <span className="text-foreground/90">{v}</span>
            </div>
          ))}
        </div>
      )}
      <div className="flex flex-col gap-3">{renderBody(body)}</div>
    </div>
  );
}
