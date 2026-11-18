import type { AuditEvent } from "$lib/model/types";
import type { UiLocale } from "$lib/i18n";
import { translator } from "$lib/i18n";
import { auditOutcomeLabel } from "./jobs";

export function logTitle(event: AuditEvent, language: UiLocale): string {
  const t = translator(language);
  const parts = event.message.split(": ");
  // New native records use a safe message catalogue. Older logs remain readable.
  if (parts.length === 2 && !parts[0].includes("_")) {
    const status: Record<string, string> = { started: "Started", completed: "Completed", failed: "Failed", queued: "Queued", cancelled: "Cancelled", "result not confirmed": "Result not confirmed" };
    return `${t(parts[0])} · ${t(status[parts[1]] ?? parts[1])}`;
  }
  if (/^[\w.]+: (Succeeded|Failed|Queued|Cancelled|Unknown)$/.test(event.message)) {
    return `${event.operation.replaceAll("_", " ")} · ${auditOutcomeLabel(event.outcome, language)}`;
  }
  return t(event.message || event.operation);
}

export function technicalEvent(event: AuditEvent): boolean {
  if (event.level !== "info" || event.outcome === "failed") return false;
  return event.kind === "started" || /^(get_|list_|player_subtitle$|control_player$)/.test(event.operation);
}

export function logFields(event: AuditEvent): [string, string][] {
  const fields: [string, string][] = [];
  function collect(value: unknown, prefix = "") {
    if (!value || typeof value !== "object" || Array.isArray(value)) return;
    for (const [key, item] of Object.entries(value)) {
      const path = prefix ? `${prefix}.${key}` : key;
      if (item !== null && typeof item === "object") collect(item, path);
      else if (item !== null && item !== undefined) fields.push([path, String(item)]);
    }
  }
  collect(event.details);
  return fields;
}

export function logMatches(event: AuditEvent, search: string, language: UiLocale = "en"): boolean {
  if (!search.trim()) return true;
  const t = translator(language);
  const text = `${JSON.stringify(event)} ${logTitle(event,language)} ${t(event.errorMessage ?? "")} ${t(event.hint ?? "")}`;
  return text.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase());
}

export function logTime(at: string, language: UiLocale): string {
  const date = new Date(at);
  return Number.isNaN(date.getTime()) ? at : new Intl.DateTimeFormat(language, {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false,
  }).format(date);
}

export function logText(event: AuditEvent): string {
  return [
    `${event.at} [${event.level.toUpperCase()}] ${event.message}`,
    `operation=${event.operation} stage=${event.stage ?? "result"}${event.durationMs != null ? ` duration=${event.durationMs}ms` : ""}`,
    event.errorMessage && `Cause: ${event.errorMessage}`,
    event.hint && `Next: ${event.hint}`,
    ...logFields(event).map(([key, value]) => `${key}=${value}`),
    event.runId && `runId=${event.runId}`, event.operationId && `operationId=${event.operationId}`, event.jobId && `jobId=${event.jobId}`,
  ].filter(Boolean).join("\n");
}

export function redundantCause(event: AuditEvent, source: AuditEvent[]): boolean {
  return event.stage === "cause" && !event.jobId && !!event.operationId && source.some((other) =>
    other.operationId === event.operationId && other.kind === "finished" && other.outcome === "failed" && !!other.details?.errorDetails
  );
}
