import { describe, expect, it, vi } from "vitest";
import type { Account } from "$lib/model/types";
import { AccountsController } from "./accounts-controller.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
const account: Account = { id: "fixture", label: "Fixture", loginName: "fixture@example.invalid", hasCredential: true, enabled: true, createdAt: "", updatedAt: "", lastLoginAt: null, lastSyncAt: null };
function fixture() {
  const notices = { success: vi.fn(), error: vi.fn() };
  const confirm = vi.fn(async () => true);
  const changed = vi.fn(async (_reset: boolean) => {});
  const api = {
    listAccounts: vi.fn(async () => [account]),
    saveAccount: vi.fn(async () => account),
    setAccountEnabled: vi.fn(async () => undefined),
    removeAccount: vi.fn(async () => ({ accountId: account.id, label: account.label, credentialDeleted: true })),
  };
  return { notices, confirm, changed, api, controller: new AccountsController(notices, confirm, changed, api) };
}

describe("account state lifecycle", () => {
  it("keeps the newest account read and clears transient passwords after save", async () => {
    const { controller, api } = fixture();
    const old = deferred<Account[]>(); api.listAccounts.mockReturnValueOnce(old.promise);
    const first = controller.load(); await controller.load();
    old.resolve([]); await first;
    expect(controller.accounts).toHaveLength(1);
    controller.edit(account); controller.password = "one-shot";
    await controller.save(new Event("submit"));
    expect(controller.password).toBe("");
    expect(controller.editingId).toBe(account.id);
  });

  it("deduplicates save submissions and discards late saves after disposal", async () => {
    const { controller, api, notices } = fixture();
    const saved = deferred<Account>(); api.saveAccount.mockReturnValueOnce(saved.promise);
    controller.password = "one-shot";
    const pending = controller.save(new Event("submit"));
    await controller.save(new Event("submit"));
    expect(api.saveAccount).toHaveBeenCalledOnce();
    controller.dispose(); saved.resolve(account); await pending;
    expect(controller.password).toBe("");
    expect(api.listAccounts).not.toHaveBeenCalled();
    expect(notices.success).not.toHaveBeenCalled();
  });

  it("does not delete after a declined or disposed confirmation", async () => {
    const { controller, api, confirm } = fixture();
    confirm.mockResolvedValueOnce(false); await controller.remove(account);
    expect(api.removeAccount).not.toHaveBeenCalled();
    const decision = deferred<boolean>(); confirm.mockReturnValueOnce(decision.promise);
    const pending = controller.remove(account); controller.dispose(); decision.resolve(true); await pending;
    expect(api.removeAccount).not.toHaveBeenCalled();
  });

  it("refreshes ownership-dependent results with the correct page policy", async () => {
    const { controller, changed } = fixture();
    await controller.setEnabled(account, false);
    expect(changed).toHaveBeenLastCalledWith(false);
    await controller.remove(account);
    expect(changed).toHaveBeenLastCalledWith(true);
  });
});
