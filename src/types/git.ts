export interface CommitInfo {
  sha: string;
  short_sha: string;
  author_name: string;
  author_email: string;
  summary: string;
  message: string;
  timestamp: string;
  files_changed: FileChange[];
}

export type FileChange =
  | { status: "added"; path: string }
  | { status: "deleted"; path: string }
  | { status: "modified"; path: string }
  | { status: "renamed"; from: string; to: string }
  | { status: "copied"; from: string; to: string }
  | { status: "typechange"; path: string }
  | { status: "other"; path: string; kind: string };
