import type { CardSummary } from "@/lib/studio-types";

// The notes tree: folders and pages exactly as they are on disk, in the
// order a person expects (folders first, then pages, each A to Z).

export type TreeNode =
  | { kind: "folder"; name: string; path: string; icon: string | null; children: TreeNode[] }
  | { kind: "page"; name: string; path: string; title: string; icon: string | null };

export function buildTree(cards: CardSummary[], folders: string[], folderIcons: Record<string, string> = {}): TreeNode[] {
  const root: TreeNode[] = [];
  const folderAt = new Map<string, Extract<TreeNode, { kind: "folder" }>>();

  const ensureFolder = (path: string): TreeNode[] => {
    if (path === "") return root;
    const existing = folderAt.get(path);
    if (existing) return existing.children;
    const slash = path.lastIndexOf("/");
    const parent = ensureFolder(slash < 0 ? "" : path.slice(0, slash));
    const node: Extract<TreeNode, { kind: "folder" }> = {
      kind: "folder",
      name: path.slice(slash + 1),
      path,
      icon: folderIcons[path] ?? null,
      children: [],
    };
    parent.push(node);
    folderAt.set(path, node);
    return node.children;
  };

  for (const folder of folders) ensureFolder(folder);
  for (const card of cards) {
    const slash = card.path.lastIndexOf("/");
    ensureFolder(slash < 0 ? "" : card.path.slice(0, slash)).push({
      kind: "page",
      name: card.path.slice(slash + 1),
      path: card.path,
      title: card.title,
      icon: card.icon ?? null,
    });
  }

  const sort = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      const an = a.kind === "page" ? a.title : a.name;
      const bn = b.kind === "page" ? b.title : b.name;
      return an.localeCompare(bn, undefined, { sensitivity: "base", numeric: true });
    });
    for (const n of nodes) if (n.kind === "folder") sort(n.children);
  };
  sort(root);
  return root;
}

export const parentOf = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
export const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);
export const join = (folder: string, name: string) => (folder ? `${folder}/${name}` : name);

/** A file or folder name from what a person typed: no slashes or other characters the disk dislikes. */
export function safeName(text: string): string {
  const cleaned = text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
  return cleaned || "untitled";
}

/** `name` (with `ext`) made unique among `taken` paths inside `folder`. */
export function uniquePath(folder: string, name: string, ext: string, taken: Set<string>): string {
  let candidate = join(folder, `${name}${ext}`);
  for (let n = 2; taken.has(candidate); n += 1) candidate = join(folder, `${name}-${n}${ext}`);
  return candidate;
}
