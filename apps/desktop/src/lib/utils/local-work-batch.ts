import type { ProductDownload } from "$lib/model/types";
import { errorMessage } from "$lib/utils/format";

export type LocalWorkOperation = "delete" | "export";
export type LocalWorkFailure = { workId: string; message: string };
export type LocalWorkProgress = {
  operation: LocalWorkOperation;
  completed: number;
  total: number;
  succeeded: string[];
  failures: LocalWorkFailure[];
};

export async function runLocalWorkBatch(
  ids: string[], operation: LocalWorkOperation, destination: string | null,
  api: {
    deleteWorkDownload: (workId: string) => Promise<ProductDownload>;
    exportWorkDownload: (workId: string, destination: string) => Promise<string>;
  },
  onProgress: (progress: LocalWorkProgress) => void,
  onDeleted: (workId: string, download: ProductDownload) => void,
) {
  if (operation === "export" && !destination) throw new Error("Export destination is required");
  const snapshot = [...new Set(ids)];
  const progress: LocalWorkProgress = { operation, completed: 0, total: snapshot.length, succeeded: [], failures: [] };
  const report = () => onProgress({ ...progress, succeeded: [...progress.succeeded], failures: [...progress.failures] });
  report();
  for (const workId of snapshot) {
    try {
      if (operation === "delete") onDeleted(workId, await api.deleteWorkDownload(workId));
      else await api.exportWorkDownload(workId, destination!);
      progress.succeeded.push(workId);
    } catch (error) {
      progress.failures.push({ workId, message: errorMessage(error) });
    }
    progress.completed++;
    report();
  }
  return progress;
}
