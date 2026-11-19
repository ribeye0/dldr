import { describe, expect, it, vi } from "vitest";
import { runLocalWorkBatch, type LocalWorkProgress } from "./local-work-batch";
import type { ProductDownload } from "$lib/model/types";

describe("local work batches", () => {
  it("continues after a delete failure, deduplicates IDs and updates only successful records", async () => {
    const download = { status: "notDownloaded" } as ProductDownload;
    const api = {
      deleteWorkDownload: vi.fn(async (id: string) => {
        if (id === "RJ000002") throw new Error("Folder is busy");
        return download;
      }),
      exportWorkDownload: vi.fn(),
    };
    const deleted = vi.fn();
    const progress: LocalWorkProgress[] = [];
    const result = await runLocalWorkBatch(["RJ000001", "RJ000002", "RJ000001", "RJ000003"], "delete", null, api, p => progress.push(p), deleted);
    expect(api.deleteWorkDownload.mock.calls.map(([id]) => id)).toEqual(["RJ000001", "RJ000002", "RJ000003"]);
    expect(deleted.mock.calls).toEqual([["RJ000001", download], ["RJ000003", download]]);
    expect(result.failures).toEqual([{ workId: "RJ000002", message: "Folder is busy" }]);
    expect(progress.map(p => p.completed)).toEqual([0, 1, 2, 3]);
    expect(progress[0].succeeded).toEqual([]);
    expect(api.exportWorkDownload).not.toHaveBeenCalled();
  });

  it("exports the selection snapshot without deleting files or changing download records", async () => {
    const ids = ["RJ000001", "RJ000002"];
    const api = {
      deleteWorkDownload: vi.fn(),
      exportWorkDownload: vi.fn(async (id: string, destination: string) => {
        ids.splice(0);
        return `${destination}/${id}`;
      }),
    };
    const deleted = vi.fn();
    const result = await runLocalWorkBatch(ids, "export", "/export 日本語", api, () => {}, deleted);
    expect(result.succeeded).toEqual(["RJ000001", "RJ000002"]);
    expect(api.exportWorkDownload.mock.calls).toEqual([["RJ000001", "/export 日本語"], ["RJ000002", "/export 日本語"]]);
    expect(api.deleteWorkDownload).not.toHaveBeenCalled();
    expect(deleted).not.toHaveBeenCalled();
    await expect(runLocalWorkBatch(["RJ000001"], "export", null, api, () => {}, deleted)).rejects.toThrow("destination");
  });
});
