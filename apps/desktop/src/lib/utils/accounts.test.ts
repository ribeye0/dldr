import { describe, expect, it } from "vitest";
import type { Account } from "$lib/model/types";
import { firstRunStep } from "./accounts";

const account = (patch: Partial<Account> = {}): Account => ({
  id: "test", label: "DLsite", loginName: "test@example.invalid", enabled: true,
  hasCredential: true, createdAt: "", updatedAt: "", lastLoginAt: null,
  lastSyncAt: null, ...patch,
});

describe("first-run setup", () => {
  it("starts with account configuration for a fresh or unusable profile", () => {
    expect(firstRunStep([], null)).toBe("account");
    expect(firstRunStep([account({ hasCredential: false })], "/library")).toBe("account");
    expect(firstRunStep([account({ enabled: false, lastSyncAt: "now" })], "/library")).toBe("account");
  });
  it("resumes saved but unsynced accounts before configuring folders", () => {
    expect(firstRunStep([account()], null)).toBe("sync");
    expect(firstRunStep([account()], "/existing-library")).toBe("sync");
  });
  it("requires a successful enabled account and resumes the missing folder step", () => {
    const synced = account({ lastSyncAt: "now" });
    expect(firstRunStep([synced], null)).toBe("folders");
    expect(firstRunStep([synced], "  ")).toBe("folders");
    expect(firstRunStep([account(), synced], "/custom-library")).toBeNull();
    // Completion depends on a successful sync, even when it returns zero works.
    expect(firstRunStep([synced], "/Downloads/dldr")).toBeNull();
  });
});
