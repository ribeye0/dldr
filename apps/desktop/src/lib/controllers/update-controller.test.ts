import { describe, expect, it, vi } from "vitest";
import { UpdateController } from "./update-controller.svelte";
import type { UpdateCheck, UpdateProgress } from "$lib/api/updates";

const release = { currentVersion:"0.1.1", version:"0.2.0", notes:"Changes" };
function fixture() {
  const api = { check:vi.fn(async ():Promise<UpdateCheck> => ({kind:"appImage", release})), install:vi.fn(async (_progress:(event:UpdateProgress)=>void) => {}) };
  return { api, controller:new UpdateController(api) };
}

describe("application updates", () => {
  it("checks without installing and allows the user to decline", async () => {
    const {api,controller}=fixture(); await controller.check();
    expect(controller.phase).toBe("available"); expect(controller.release?.version).toBe("0.2.0");
    expect(api.install).not.toHaveBeenCalled(); controller.close(); expect(controller.open).toBe(false);
  });
  it("reports current and unsupported builds separately", async () => {
    const {api,controller}=fixture(); api.check.mockResolvedValueOnce({kind:"appImage",release:null});
    await controller.check(); expect(controller.phase).toBe("current");
    api.check.mockResolvedValueOnce({kind:"unsupported",release:null}); await controller.check();
    expect(controller.phase).toBe("unsupported"); await controller.install(); expect(api.install).not.toHaveBeenCalled();
  });
  it("keeps duplicate clicks and closing from interrupting installation", async () => {
    const {api,controller}=fixture(); await controller.check();
    let finish!:()=>void;
    api.install.mockImplementation(async progress => {
      progress({event:"progress",downloaded:10,total:null});
      expect(controller.received).toBe(10);expect(controller.total).toBeNull();
      progress({event:"progress",downloaded:20,total:100});
      progress({event:"progress",downloaded:15,total:100});expect(controller.received).toBe(20);
      progress({event:"verifying"});expect(controller.phase).toBe("verifying");
      await new Promise<void>(resolve=>finish=resolve);
      progress({event:"installing"});
    });
    const installing=controller.install(); controller.close(); expect(controller.open).toBe(true);
    await controller.install(); await controller.check(); expect(api.install).toHaveBeenCalledTimes(1);expect(api.check).toHaveBeenCalledTimes(1);
    finish();await installing;expect(controller.phase).toBe("installed");
  });
  it("requires a fresh check after failures and never treats a bad signature as installed", async () => {
    const {api,controller}=fixture(); await controller.check();
    api.install.mockRejectedValueOnce("update_signature [diagnostic:run-test:op-test]");await controller.install();
    expect(controller.phase).toBe("error");expect(controller.error).toMatch(/signature/);
    await controller.install();expect(api.install).toHaveBeenCalledTimes(1);
    await controller.check();await controller.install();expect(controller.phase).toBe("installed");
  });
  it("reports active jobs and unavailable feeds without claiming the app is current", async () => {
    const {api,controller}=fixture();api.check.mockRejectedValueOnce("update_feed_unavailable");
    await controller.check();expect(controller.phase).toBe("error");expect(controller.error).toMatch(/not available/);
    await controller.check();api.install.mockRejectedValueOnce("update_jobs_active");await controller.install();expect(controller.error).toMatch(/pause downloads/);
  });
  it("ignores a response after disposal", async () => {
    const {api,controller}=fixture();let finish!:(result:UpdateCheck)=>void;
    api.check.mockImplementation(()=>new Promise(resolve=>finish=resolve));
    const checking=controller.check();controller.dispose();finish({kind:"appImage",release});await checking;
    expect(controller.release).toBeNull();expect(api.install).not.toHaveBeenCalled();
  });
});
