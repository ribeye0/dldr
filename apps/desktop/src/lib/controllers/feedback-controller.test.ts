import { afterEach, describe, expect, it, vi } from "vitest";
import { DecisionController, NotificationController } from "./feedback-controller.svelte";

afterEach(() => vi.useRealTimers());

describe("transient feedback lifecycle", () => {
  it("bounds notices and timers together, including manual dismissal", () => {
    vi.useFakeTimers();
    const notices = new NotificationController();
    for (let i = 0; i < 8; i++) notices.info(String(i));
    expect(notices.toasts.map(toast => toast.message)).toEqual(["7", "6", "5", "4", "3"]);
    expect(vi.getTimerCount()).toBe(5);
    notices.dismiss(notices.toasts[0].id);
    expect(vi.getTimerCount()).toBe(4);
    vi.advanceTimersByTime(3600);
    expect(notices.toasts).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps errors longer and ignores late async notices after disposal", () => {
    vi.useFakeTimers();
    const notices = new NotificationController();
    notices.success("saved"); notices.error("failed");
    vi.advanceTimersByTime(3600);
    expect(notices.toasts.map(toast => toast.message)).toEqual(["failed"]);
    notices.dispose(); notices.error("late failure");
    expect(notices.toasts).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("settles replaced, accepted and disposed confirmations exactly once", async () => {
    const decisions = new DecisionController<{ title: string }>();
    const first = decisions.show({ title: "first" });
    const next = decisions.show({ title: "second" });
    expect(await first).toBe(false);
    expect(decisions.dialog?.title).toBe("second");
    decisions.close(true); decisions.close();
    expect(await next).toBe(true);
    const pending = decisions.show({ title: "third" });
    decisions.dispose();
    expect(await pending).toBe(false);
    expect(await decisions.show({ title: "late" })).toBe(false);
    expect(decisions.dialog).toBeNull();
  });
});
