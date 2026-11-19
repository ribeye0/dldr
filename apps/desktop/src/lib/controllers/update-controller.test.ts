import { describe, expect, it, vi } from "vitest";
import { UpdateController } from "./update-controller.svelte";
import type { ReadyPackage, UpdateCheck, UpdateProgress } from "$lib/api/updates";

const release = { currentVersion: "0.1.4", version: "0.2.0", notes: "Changes" };
const ready: ReadyPackage = { release, fileName: "DLDR_0.2.0_amd64.AppImage", size: 200, source: "online", direction: "upgrade", verified: true };
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fixture() {
  const api = {
    check: vi.fn(async (): Promise<UpdateCheck> => ({ kind: "appImage", release })),
    download: vi.fn(async (_id: string, _progress: (event: UpdateProgress) => void): Promise<ReadyPackage> => ready),
    cancel: vi.fn(async (_id: string) => {}),
    prepareLocal: vi.fn(async (_id: string, _path: string, _version: string): Promise<ReadyPackage> => ({ ...ready, source: "local", verified: false })),
    install: vi.fn(async (_trust: boolean) => {}),
    chooseFile: vi.fn(async (): Promise<string | null> => "/tmp/test.AppImage"),
  };
  const onReady = vi.fn();
  return { api, onReady, controller: new UpdateController(api, onReady) };
}

describe("application updates", () => {
  it("separates checking, downloading and explicit installation", async () => {
    const { api, controller } = fixture();
    await controller.check();
    expect(controller.phase).toBe("available");
    expect(api.download).not.toHaveBeenCalled();
    await controller.install();
    expect(api.install).not.toHaveBeenCalled();
    await controller.download();
    expect(controller.phase).toBe("ready");
    expect(api.install).not.toHaveBeenCalled();
    await controller.install();
    expect(controller.phase).toBe("installed");
    expect(api.install).toHaveBeenCalledWith(false);
  });

  it("keeps a hidden download alive and reopens its progress without rechecking", async () => {
    const { api, onReady, controller } = fixture();
    const result = deferred<ReadyPackage>();
    let progress!: (event: UpdateProgress) => void;
    api.download.mockImplementation((_id, onProgress) => { progress = onProgress; return result.promise; });
    await controller.check();
    const downloading = controller.download();
    await Promise.resolve();
    progress({ event: "progress", downloaded: 20, total: 100 });
    progress({ event: "progress", downloaded: 15, total: 100 });
    expect(controller.received).toBe(20);
    controller.close();
    expect(controller.open).toBe(false);
    expect(api.cancel).not.toHaveBeenCalled();
    await controller.check();
    expect(controller.open).toBe(true);
    expect(api.check).toHaveBeenCalledTimes(1);
    controller.close();
    progress({ event: "verifying" });
    expect(controller.phase).toBe("verifying");
    result.resolve(ready);
    await downloading;
    expect(controller.phase).toBe("ready");
    expect(controller.open).toBe(false);
    expect(onReady).toHaveBeenCalledTimes(1);
    expect(controller.statusLabel).toBe("Update ready");
    await controller.check();
    expect(api.check).toHaveBeenCalledTimes(1);
    expect(controller.ready).toEqual(ready);
  });

  it("cancels before admission and ignores progress and success racing with cancellation", async () => {
    const { api, controller } = fixture();
    const result = deferred<ReadyPackage>();
    api.download.mockImplementation(async (_id, progress) => {
      progress({ event: "progress", downloaded: 10, total: 100 });
      return result.promise;
    });
    await controller.check();
    const downloading = controller.download();
    const cancelling = controller.cancel();
    expect(controller.phase).toBe("cancelling");
    await controller.download();
    await controller.check();
    result.resolve(ready);
    await Promise.all([downloading, cancelling]);
    expect(api.cancel).toHaveBeenCalledWith(api.download.mock.calls[0][0]);
    expect(controller.ready).toBeNull();
    expect(controller.received).toBe(0);
    expect(controller.phase).toBe("available");
    api.download.mockResolvedValueOnce(ready);
    await controller.download();
    expect(controller.phase).toBe("ready");
    expect(api.download.mock.calls[0][0]).not.toBe(api.download.mock.calls[1][0]);
  });

  it("keeps a prepared package when active work prevents installation", async () => {
    const { api, controller } = fixture();
    await controller.check();
    await controller.download();
    api.install.mockRejectedValueOnce("update_jobs_active [diagnostic:run:op]");
    await controller.install();
    expect(controller.phase).toBe("ready");
    expect(controller.error).toMatch(/pause downloads/);
    await controller.install();
    expect(controller.phase).toBe("installed");
    expect(api.download).toHaveBeenCalledTimes(1);
  });

  it("rejects a bad signature and never calls the installer", async () => {
    const { api, controller } = fixture();
    await controller.check();
    api.download.mockRejectedValueOnce("update_signature");
    await controller.download();
    expect(controller.phase).toBe("error");
    expect(controller.error).toMatch(/signature/);
    expect(controller.ready).toBeNull();
    await controller.install();
    expect(api.install).not.toHaveBeenCalled();
  });

  it("requires explicit trust for local unsigned builds and forwards it to native code", async () => {
    const { api, controller } = fixture();
    await controller.chooseLocal();
    expect(controller.phase).toBe("local");
    controller.localVersion = "0.1.2";
    api.prepareLocal.mockResolvedValueOnce({ ...ready, release: { ...release, version: "0.1.2" }, source: "local", direction: "downgrade", verified: false });
    await controller.prepareLocal();
    expect(controller.ready?.direction).toBe("downgrade");
    await controller.install();
    expect(api.install).not.toHaveBeenCalled();
    expect(controller.error).toMatch(/trust/);
    controller.trustLocal = true;
    await controller.install();
    expect(api.install).toHaveBeenCalledWith(true);
    expect(api.prepareLocal.mock.calls[0].slice(1)).toEqual(["/tmp/test.AppImage", "0.1.2"]);
  });

  it("preserves ready state when the local file picker is cancelled", async () => {
    const { api, controller } = fixture();
    await controller.check();
    await controller.download();
    api.chooseFile.mockResolvedValueOnce(null);
    await controller.chooseLocal();
    expect(controller.phase).toBe("ready");
    expect(controller.ready).toEqual(ready);
  });

  it("cannot dismiss or duplicate an active installation", async () => {
    const { api, controller } = fixture();
    const result = deferred<void>();
    api.install.mockReturnValueOnce(result.promise);
    await controller.check();
    await controller.download();
    const installing = controller.install();
    controller.close();
    expect(controller.open).toBe(true);
    await controller.install();
    await controller.check();
    await controller.chooseLocal();
    expect(api.install).toHaveBeenCalledTimes(1);
    expect(api.chooseFile).not.toHaveBeenCalled();
    result.resolve();
    await installing;
    expect(controller.phase).toBe("installed");
  });

  it("reports current and unsupported builds separately", async () => {
    const { api, controller } = fixture();
    api.check.mockResolvedValueOnce({ kind: "appImage", release: null });
    await controller.check();
    expect(controller.phase).toBe("current");
    api.check.mockResolvedValueOnce({ kind: "unsupported", release: null });
    await controller.check();
    expect(controller.phase).toBe("unsupported");
    await controller.install();
    expect(api.install).not.toHaveBeenCalled();
  });

  it("cancels transfers on disposal and ignores late responses", async () => {
    const { api, onReady, controller } = fixture();
    const result = deferred<ReadyPackage>();
    api.download.mockReturnValueOnce(result.promise);
    await controller.check();
    const downloading = controller.download();
    controller.dispose();
    result.resolve(ready);
    await downloading;
    expect(api.cancel).toHaveBeenCalledTimes(1);
    expect(controller.ready).toBeNull();
    expect(onReady).not.toHaveBeenCalled();
  });
});
