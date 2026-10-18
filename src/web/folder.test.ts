import { describe, expect, it } from "vitest";
import { folderFromDrop, folderFromInput } from "./folder";

function picked(path: string, text = "x"): File {
  const file = new File([text], path.split("/").at(-1)!);
  Object.defineProperty(file, "webkitRelativePath", { value: path });
  return file;
}

interface FakeEntry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  file?: (success: (file: File) => void) => void;
  createReader?: () => { readEntries: (success: (entries: FakeEntry[]) => void) => void };
}

function fileEntry(name: string): FakeEntry {
  return { isFile: true, isDirectory: false, name, file: (success) => success(new File(["x"], name)) };
}

function directoryEntry(name: string, children: FakeEntry[]): FakeEntry {
  return {
    isFile: false,
    isDirectory: true,
    name,
    createReader: () => {
      let read = false;
      // Browsers return entries in batches; the last call returns none.
      return { readEntries: (success) => { success(read ? [] : children); read = true; } };
    },
  };
}

function dropped(entries: FakeEntry[]): DataTransferItemList {
  return entries.map((entry) => ({ kind: "file", webkitGetAsEntry: () => entry })) as unknown as DataTransferItemList;
}

describe("dropped and picked folders", () => {
  it("strips the folder name and skips repository internals", () => {
    const selection = folderFromInput([picked("my_mod/descriptor.mod"), picked("my_mod/.git/HEAD"), picked("my_mod/events/a.txt", "abc")]);
    expect(selection.name).toBe("my_mod");
    expect(selection.files.map((file) => file.path)).toEqual(["descriptor.mod", "events/a.txt"]);
    expect(selection.skipped).toEqual([".git"]);
    expect(selection.totalBytes).toBe(4);
  });

  it("walks one dropped folder recursively", async () => {
    const root = directoryEntry("mod", [fileEntry("descriptor.mod"), directoryEntry("common", [fileEntry("ideas.txt")]), directoryEntry("node_modules", [fileEntry("x.js")])]);
    const selection = await folderFromDrop(dropped([root]));
    expect(selection.files.map((file) => file.path)).toEqual(["common/ideas.txt", "descriptor.mod"]);
    expect(selection.skipped).toEqual(["node_modules"]);
  });

  it("refuses loose files or several folders", async () => {
    await expect(folderFromDrop(dropped([fileEntry("a.txt")]))).rejects.toThrow(/one mod folder/);
    await expect(folderFromDrop(dropped([directoryEntry("a", []), directoryEntry("b", [])]))).rejects.toThrow(/one mod folder/);
  });
});
