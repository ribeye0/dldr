import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { diagnosticInvoke, observeNative } from "./diagnostics";

export type UpdateRelease = { currentVersion: string; version: string; notes: string };
export type UpdateCheck = {
  kind: "appImage" | "windowsInstaller" | "windowsPortable" | "unsupported";
  release: UpdateRelease | null;
};
export type ReadyPackage = {
  release: UpdateRelease;
  fileName: string;
  size: number;
  source: "online" | "local";
  direction: "upgrade" | "downgrade" | "reinstall";
  verified: boolean;
};
export type UpdateProgress =
  | { event: "progress"; downloaded: number; total: number | null }
  | { event: "verifying" };

export const check = () => diagnosticInvoke<UpdateCheck>("check_app_update");
export function download(transferId: string, onProgress: (event: UpdateProgress) => void) {
  const onEvent = new Channel<UpdateProgress>();
  onEvent.onmessage = onProgress;
  return diagnosticInvoke<ReadyPackage>("download_app_update", { transferId, onEvent });
}
export const cancel = (transferId: string) => diagnosticInvoke<void>("cancel_app_update", { transferId });
export const prepareLocal = (transferId: string, path: string, version: string) =>
  diagnosticInvoke<ReadyPackage>("prepare_local_app_update", { transferId, path, version: version || null });
export const install = (trustLocal: boolean) => diagnosticInvoke<void>("install_app_update", { trustLocal });
export const chooseFile = () => observeNative("native.dialog", () => open({
  directory: false, multiple: false,
  filters: [{ name: "DLDR", extensions: ["AppImage", "appimage", "zip", "exe"] }],
})) as Promise<string | null>;
