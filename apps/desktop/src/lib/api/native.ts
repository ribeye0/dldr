import { get } from "svelte/store";
import { t } from "$lib/i18n";
import { getIdentifier, getName, getTauriVersion, getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { downloadDir } from "@tauri-apps/api/path";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { diagnosticInvoke, observeNative } from "./diagnostics";
import { relaunch } from "@tauri-apps/plugin-process";
import type { AppInfo, JobEvent, TwoFactorClosed, TwoFactorRequest } from "$lib/model/types";

export type DirectorySelection = {
  title: string;
  defaultPath?: string;
  canCreateDirectories: boolean;
};

export async function getAppInfo(): Promise<AppInfo> {
  const [name, version, identifier, tauriVersion] = await observeNative("native.appInfo", () => Promise.all([
    getName(),
    getVersion(),
    getIdentifier(),
    getTauriVersion(),
  ]));

  return { name, version, identifier, tauriVersion };
}

export function getSystemDownloadDirectory() {
  return observeNative("native.path", () => downloadDir());
}

export function chooseDirectory(options: DirectorySelection) {
  return observeNative("native.dialog", () => openDialog({
    directory: true,
    multiple: false,
    canCreateDirectories: options.canCreateDirectories,
    defaultPath: options.defaultPath,
    title: options.title,
  }));
}

export function openExternalUrl(url: string) {
  return diagnosticInvoke<void>("open_external_url", { url });
}

export function chooseBackupDestination() {
  return observeNative("native.dialog", () => saveDialog({ title: get(t)("匯出作品庫備份（請使用新檔名）"), defaultPath: `dlsite-library-${new Date().toISOString().replace(/[:.]/g,"-")}.json`, filters: [{name:get(t)("Library backup"),extensions:["json"]}] }));
}
export function chooseBackupFile() {
  return observeNative("native.dialog", () => openDialog({ title:get(t)("選擇作品庫備份"), directory:false, multiple:false, filters:[{name:get(t)("Library backup"),extensions:["json"]}] })) as Promise<string|null>;
}

export function relaunchApp() {
  return observeNative("native.relaunch", () => relaunch());
}

export function listenToJobEvents(handler: (event: JobEvent) => void) {
  return observeNative("native.listener", () => listen<JobEvent>("dldr-job-event", (event) => handler(event.payload)));
}

export function listenToTwoFactorRequests(handler: (request: TwoFactorRequest) => void) {
  return observeNative("native.listener", () => listen<TwoFactorRequest>("dldr-two-factor-request", (event) => handler(event.payload)));
}

export function listenToTwoFactorClosures(handler: (closed: TwoFactorClosed) => void) {
  return observeNative("native.listener", () => listen<TwoFactorClosed>("dldr-two-factor-closed", (event) => handler(event.payload)));
}

export function openDlsitePlay(accountId: string, workId: string | null = null) {
  return diagnosticInvoke<void>("open_dlsite_play", {accountId, workId});
}

export function setNativeHeaderTheme(dark: boolean) {
  return diagnosticInvoke<void>("set_native_header_theme", { dark });
}
