import { describe, it, expect, vi } from "vitest";
import type { Product, ProductDetail } from "$lib/model/types";
import { CreditHydrationController } from "./credit-hydration-controller";

function work(workId: string, workType = "SOU", voices: string[] = []): Product {
  return { workId, workType, creditGroups: voices.length ? [{ kind: "voice", label: "CV", names: voices }] : [] } as Product;
}
function deferred() {
  let resolve!: (value: ProductDetail) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<ProductDetail>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const detail = (id: string) => ({ workId: id, creditGroups: [{ kind: "voice", label: "CV", names: ["伊倉える", "浅木式"] }] }) as ProductDetail;

describe("missing voice metadata hydration", () => {
  it("limits requests across pages, discards queued old works and keeps both actors", async () => {
    const requests = new Map<string, ReturnType<typeof deferred>>();
    const port = { getProductDetail: vi.fn((id: string) => { const task = deferred(); requests.set(id, task); return task.promise; }) };
    const view = { updated: vi.fn(), pending: vi.fn() };
    const controller = new CreditHydrationController(port, view);
    controller.show([work("a"), work("b"), work("old-queued")]);
    expect(port.getProductDetail.mock.calls.flat()).toEqual(["a", "b"]);
    controller.show([work("a"), work("new-page")]);
    requests.get("a")!.resolve(detail("a"));
    await vi.waitFor(() => expect(port.getProductDetail.mock.calls.flat()).toEqual(["a", "b", "new-page"]));
    expect(view.updated).toHaveBeenCalledWith(detail("a"));
    expect(port.getProductDetail).not.toHaveBeenCalledWith("old-queued");
    requests.get("b")!.resolve(detail("b"));
    requests.get("new-page")!.resolve(detail("new-page"));
    await vi.waitFor(() => expect(view.pending).toHaveBeenLastCalledWith([]));
    controller.show([work("a"), work("new-page")]);
    expect(port.getProductDetail).toHaveBeenCalledTimes(3);
  });

  it("skips non-audio and provided voices, and allows Reload to retry a failure", async () => {
    const port = { getProductDetail: vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(detail("missing")) };
    const view = { updated: vi.fn(), pending: vi.fn() };
    const controller = new CreditHydrationController(port, view);
    const works = [work("image", "COM"), work("complete", "SOU", ["Voice"]), work("missing")];
    controller.show(works);
    await vi.waitFor(() => expect(view.pending).toHaveBeenLastCalledWith([]));
    controller.show(works);
    expect(port.getProductDetail).toHaveBeenCalledExactlyOnceWith("missing");
    controller.retry();
    controller.show(works);
    await vi.waitFor(() => expect(view.updated).toHaveBeenCalledWith(detail("missing")));
  });

  it("does not update a disposed view or dispatch more queued requests", async () => {
    const task = deferred();
    const port = { getProductDetail: vi.fn().mockReturnValue(task.promise) };
    const view = { updated: vi.fn(), pending: vi.fn() };
    const controller = new CreditHydrationController(port, view);
    controller.show([work("a"), work("b"), work("c")]);
    controller.dispose();
    task.resolve(detail("a"));
    await task.promise;
    expect(view.updated).not.toHaveBeenCalled();
    expect(port.getProductDetail).toHaveBeenCalledTimes(2);
  });
});
