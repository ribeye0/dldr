import { Channel } from "@tauri-apps/api/core";
import { diagnosticInvoke } from "./diagnostics";

export type UpdateRelease = { currentVersion: string; version: string; notes: string };
export type UpdateCheck = {
  kind: "appImage" | "windowsInstaller" | "windowsPortable" | "unsupported";
  release: UpdateRelease | null;
};
export type UpdateProgress =
  | { event: "progress"; downloaded: number; total: number | null }
  | { event: "verifying" | "installing" };

export const check = () => diagnosticInvoke<UpdateCheck>("check_app_update");
export function install(onProgress: (event: UpdateProgress) => void) {
  const onEvent = new Channel<UpdateProgress>();
  onEvent.onmessage = onProgress;
  return diagnosticInvoke<void>("install_app_update", { onEvent });
}
