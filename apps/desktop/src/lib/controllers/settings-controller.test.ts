import { afterEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { locale } from "$lib/i18n";
import type { AppSettings } from "$lib/model/types";
import type { SaveSettingsRequest } from "$lib/api/tauri";
import { SettingsController } from "./settings-controller.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
const settings = (overrides: Partial<AppSettings> = {}): AppSettings => ({
  autoExtract: true, uiLocale: "en", libraryRoot: "/library", downloadRoot: null,
  languagePriority: ["en_US", "ja_JP", "zh_TW", "zh_CN", "und"], titleLanguageFollowsUi: true, ...overrides,
});
function fixture() {
  const notices = { success: vi.fn(), error: vi.fn() };
  const changed = vi.fn(async () => {});
  const api = {
    getSettings: vi.fn(async () => settings()),
    saveSettings: vi.fn(async (_request: SaveSettingsRequest) => settings()),
    getSystemDownloadDirectory: vi.fn(async () => "/downloads/dldr"),
    getAppInfo: vi.fn(async () => ({ name: "DLDR", version: "0.1.4", identifier: "dldr", tauriVersion: "2" })),
    chooseDirectory: vi.fn(async () => "/chosen"),
  };
  return { api, notices, changed, controller: new SettingsController(notices, changed, api) };
}
afterEach(() => locale.set("en"));

describe("settings service boundary", () => {
  it("ignores out-of-order loads and responses after disposal", async () => {
    const { api, controller, notices } = fixture();
    const older = deferred<AppSettings>();
    api.getSettings.mockReturnValueOnce(older.promise);
    const first = controller.load();
    await controller.load();
    older.resolve(settings({ libraryRoot: "/stale", uiLocale: "ja" }));
    expect(await first).toBe(false);
    expect(controller.libraryRoot).toBe("/library");
    expect(get(locale)).toBe("en");
    const pending = deferred<AppSettings>();
    api.getSettings.mockReturnValueOnce(pending.promise);
    const last = controller.load(); controller.dispose();
    pending.resolve(settings({ libraryRoot: "/disposed" })); await last;
    expect(controller.libraryRoot).toBe("/library");
    expect(notices.error).not.toHaveBeenCalled();
  });

  it("separates draft extraction settings from the saved download policy", async () => {
    const { api, controller, changed } = fixture();
    await controller.load();
    expect(controller.downloadRoot).toBe("/downloads/dldr");
    controller.autoExtract = false;
    expect(controller.unpackPolicy).toBe("unpackWhenRecognized");
    api.saveSettings.mockResolvedValueOnce(settings({ autoExtract: false, downloadRoot: "/staging" }));
    await controller.save(new Event("submit"));
    expect(controller.unpackPolicy).toBe("keepArchives");
    expect(controller.configuredDownloadRoot).toBe("/staging");
    expect(changed).not.toHaveBeenCalled();
  });

  it("invalidates title projections before waiting for directory lookup and prevents double saves", async () => {
    const { api, controller, changed } = fixture();
    await controller.load();
    const saving = deferred<AppSettings>();
    const directory = deferred<string>();
    api.saveSettings.mockReturnValueOnce(saving.promise);
    api.getSystemDownloadDirectory.mockReturnValueOnce(directory.promise);
    controller.titleLanguageFollowsUi = false;
    controller.draftLanguagePriority = ["ja_JP", "en_US", "zh_TW", "zh_CN", "und"];
    const pending = controller.save(new Event("submit"));
    await controller.save(new Event("submit"));
    expect(api.saveSettings).toHaveBeenCalledTimes(1);
    expect(api.saveSettings.mock.calls[0][0]).toMatchObject({ languagePriority: ["ja_JP", "en_US", "zh_TW", "zh_CN", "und"] });
    saving.resolve(settings({ languagePriority: ["ja_JP", "en_US", "zh_TW", "zh_CN", "und"], titleLanguageFollowsUi: false }));
    await vi.waitFor(() => expect(changed).toHaveBeenCalledOnce());
    expect(controller.saving).toBe(true);
    directory.resolve("/new-default"); await pending;
    expect(controller.downloadRoot).toBe("/new-default");
  });

  it("keeps directory cancellation harmless and suppresses a late directory selection", async () => {
    const { api, controller } = fixture();
    await controller.load();
    const pending = deferred<string>();
    api.chooseDirectory.mockReturnValueOnce(pending.promise);
    const choosing = controller.chooseDirectory("library");
    await vi.waitFor(() => expect(api.chooseDirectory).toHaveBeenCalledOnce());
    controller.dispose(); pending.resolve("/late"); await choosing;
    expect(controller.libraryRoot).toBe("/library");
  });
});
