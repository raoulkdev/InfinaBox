export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  /** Present (possibly empty) for directories, `null` for files. */
  children: FileEntry[] | null;
}
