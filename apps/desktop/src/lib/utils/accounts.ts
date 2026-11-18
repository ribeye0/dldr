import type { Account } from "$lib/model/types";
import { shortDate } from "$lib/utils/format";

export function accountLoginLabel(account: Account) {
  return account.loginName?.trim() || "No login name";
}

export function accountLastSyncLabel(account: Account) {
  return account.lastSyncAt ? shortDate(account.lastSyncAt) : "Never synced";
}

export function accountCredentialLabel(account: Account) {
  return account.hasCredential ? "Saved" : "Not saved";
}

export function accountEnabledLabel(account: Account) {
  return account.enabled ? "Enabled" : "Disabled";
}

export function enabledAccountCount(accounts: Account[]) {
  return accounts.filter((account) => account.enabled).length;
}

export function credentialedAccountCount(accounts: Account[]) {
  return accounts.filter((account) => account.hasCredential).length;
}
// Read persisted configuration, so interrupted setup resumes without a second
// completion flag that could disagree with the actual account or folder state.
export function firstRunStep(accounts: Account[], libraryRoot: string | null): "account" | "sync" | "folders" | null {
  const ready = accounts.filter(account => account.enabled && account.hasCredential);
  if (!ready.length) return "account";
  if (!ready.some(account => account.lastSyncAt)) return "sync";
  return libraryRoot?.trim() ? null : "folders";
}
