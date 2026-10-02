import { contextRead, contextWrite, documentImport } from "@/lib/studio-api";
import type { CardMeta } from "@/lib/studio-types";
import { DOC_TEMPLATES, templateById, templatedCard } from "@/components/context/templates";
import { safeName, uniquePath } from "./paths";
import type { Block, Boards } from "./types";

// Documents are ordinary Markdown cards in `.ibproject/context/`; these make
// and copy them for the board.

const plainMeta = (title: string): CardMeta => ({ type: null, title, status: null, links: [], implemented_in: [], tags: [], extra: {} });

/** A new document (blank, or from a template); returns its path. */
export async function createDocument(projectPath: string, taken: Set<string>, template: string | null): Promise<string> {
  if (template && DOC_TEMPLATES.some((t) => t.id === template)) {
    const t = templateById(template);
    const made = templatedCard(t, t.label, taken);
    const path = uniquePath(safeName(t.label), ".md", taken);
    await contextWrite(projectPath, path, made.meta, made.body);
    return path;
  }
  const path = uniquePath("untitled", ".md", taken);
  await contextWrite(projectPath, path, plainMeta("Untitled"), "");
  return path;
}

/** A Markdown file dropped on the board becomes a document. */
export async function importMarkdown(projectPath: string, fileName: string, text: string, taken: Set<string>): Promise<string> {
  const title = fileName.replace(/\.(md|markdown)$/i, "").trim() || "Untitled";
  const path = uniquePath(safeName(title), ".md", taken);
  await contextWrite(projectPath, path, plainMeta(title), text);
  return path;
}

/** Files whose text can be read into a document. */
export const IMPORTABLE = /\.(pdf|docx|xlsx|xlsm|xls|ods|csv|tsv)$/i;

/** A PDF, Word file, spreadsheet or CSV becomes a document; the original is
 * kept and named at the top. Returns the new document's path. */
export async function importDocument(projectPath: string, file: File, dataBase64: string, taken: Set<string>): Promise<string> {
  const imported = await documentImport(projectPath, file.name, dataBase64);
  const path = uniquePath(safeName(imported.title), ".md", taken);
  const head = `*Imported from ${file.name}* (original kept at \`${imported.source}\`)\n\n`;
  const note = imported.note ? `\n\n*${imported.note}*\n` : "";
  await contextWrite(projectPath, path, plainMeta(imported.title), head + imported.markdown + note);
  return path;
}

/** Every document the blocks (and boards nested in them) point at. */
export function collectDocRefs(blocks: Block[], boards: Boards, seen = new Set<string>()): string[] {
  const out: string[] = [];
  for (const b of blocks) {
    if (b.type === "doc" && b.ref && !out.includes(b.ref)) out.push(b.ref);
    if (b.type === "board" && b.ref && !seen.has(b.ref) && boards[b.ref]) {
      seen.add(b.ref);
      for (const r of collectDocRefs(boards[b.ref]!.blocks, boards, seen)) if (!out.includes(r)) out.push(r);
    }
  }
  return out;
}

/** Copies documents as new cards; returns old path → copy's path. */
export async function copyDocs(projectPath: string, refs: string[], taken: Set<string>): Promise<Map<string, string>> {
  const map = new Map<string, string>();
  for (const ref of refs) {
    try {
      const card = await contextRead(projectPath, ref);
      const title = `${card.meta.title || ref.replace(/\.md$/i, "").split("/").pop() || "Untitled"} copy`;
      const base = ref.replace(/\.md$/i, "").split("/").pop() || "untitled";
      const path = uniquePath(`${safeName(base)}-copy`, ".md", taken);
      taken.add(path);
      await contextWrite(projectPath, path, { ...card.meta, title, links: [...card.meta.links] }, card.body);
      map.set(ref, path);
    } catch {
      // A document that can't be read keeps pointing at the original.
    }
  }
  return map;
}
