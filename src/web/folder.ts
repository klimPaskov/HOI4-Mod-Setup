// Read a mod folder the user drops on the page or picks with the folder
// picker. Files stay in the browser; this only lists them and reads bytes.

export interface FolderFile {
  /** Path relative to the dropped folder, with `/` separators. */
  path: string;
  file: File;
}

export interface FolderSelection {
  name: string;
  files: FolderFile[];
  skipped: string[];
  totalBytes: number;
}

/** Folders that never matter for setup and can be very large. */
const SKIPPED_FOLDERS = new Set([".git", ".hg", ".svn", "node_modules", ".venv", "venv", "__pycache__"]);
export const MAX_FILES = 200_000;
export const MAX_TOTAL_BYTES = 2 * 1024 * 1024 * 1024;

export class FolderTooLargeError extends Error {
  constructor() {
    super("This folder is too large to prepare in the browser (more than 200,000 files or 2 GB). Use the desktop app for it.");
  }
}

function skipped(segments: string[]): boolean {
  return segments.some((segment) => SKIPPED_FOLDERS.has(segment.toLowerCase()));
}

function accept(selection: FolderSelection, path: string, file: File): void {
  const segments = path.split("/");
  if (skipped(segments.slice(0, -1))) {
    const folder = segments.find((segment) => SKIPPED_FOLDERS.has(segment.toLowerCase()));
    if (folder && !selection.skipped.includes(folder)) selection.skipped.push(folder);
    return;
  }
  selection.files.push({ path, file });
  selection.totalBytes += file.size;
  if (selection.files.length > MAX_FILES || selection.totalBytes > MAX_TOTAL_BYTES) throw new FolderTooLargeError();
}

/** Files chosen with `<input type="file" webkitdirectory>`. */
export function folderFromInput(list: FileList | File[]): FolderSelection {
  const files = Array.from(list);
  const first = (files[0] as File & { webkitRelativePath?: string } | undefined)?.webkitRelativePath ?? "";
  const name = first.split("/")[0] ?? "";
  const selection: FolderSelection = { name, files: [], skipped: [], totalBytes: 0 };
  for (const file of files) {
    const relative = (file as File & { webkitRelativePath?: string }).webkitRelativePath ?? file.name;
    const path = relative.split("/").slice(1).join("/");
    if (path) accept(selection, path, file);
  }
  selection.files.sort((left, right) => left.path.localeCompare(right.path));
  return selection;
}

interface EntryLike {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  file?: (success: (file: File) => void, failure: (error: unknown) => void) => void;
  createReader?: () => { readEntries: (success: (entries: EntryLike[]) => void, failure: (error: unknown) => void) => void };
}

function readAll(directory: EntryLike): Promise<EntryLike[]> {
  const reader = directory.createReader!();
  const entries: EntryLike[] = [];
  return new Promise((resolve, reject) => {
    const next = () => reader.readEntries((batch) => {
      if (batch.length === 0) resolve(entries);
      else {
        entries.push(...batch);
        next();
      }
    }, reject);
    next();
  });
}

function fileOf(entry: EntryLike): Promise<File> {
  return new Promise((resolve, reject) => entry.file!(resolve, reject));
}

/** A folder dropped on the page. Exactly one folder must be dropped. */
export async function folderFromDrop(items: DataTransferItemList): Promise<FolderSelection> {
  const entries = Array.from(items)
    .filter((item) => item.kind === "file")
    .map((item) => (item as DataTransferItem & { webkitGetAsEntry?: () => EntryLike | null }).webkitGetAsEntry?.() ?? null)
    .filter((entry): entry is EntryLike => entry !== null);
  const directories = entries.filter((entry) => entry.isDirectory);
  if (directories.length !== 1 || entries.length !== 1) {
    throw new Error("Drop one mod folder, not single files or several folders.");
  }
  const root = directories[0];
  const selection: FolderSelection = { name: root.name, files: [], skipped: [], totalBytes: 0 };
  const visit = async (directory: EntryLike, prefix: string): Promise<void> => {
    for (const entry of await readAll(directory)) {
      const path = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory) {
        if (SKIPPED_FOLDERS.has(entry.name.toLowerCase())) {
          if (!selection.skipped.includes(entry.name)) selection.skipped.push(entry.name);
          continue;
        }
        await visit(entry, path);
      } else if (entry.isFile) {
        accept(selection, path, await fileOf(entry));
      }
    }
  };
  await visit(root, "");
  selection.files.sort((left, right) => left.path.localeCompare(right.path));
  return selection;
}
