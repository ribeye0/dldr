import { describe, it, expect } from "vitest";
import { logTitle, logMatches, logText, technicalEvent, redundantCause } from "./logs";
import type { AuditEvent } from "$lib/model/types";
const event: AuditEvent = {
  at:"2026-11-18T09:15:32.135Z", level:"error", operation:"start_work_download", kind:"diagnostic", stage:"cause",
  outcome:"failed", message:"Queue download: failed", errorCode:"storage",
  errorMessage:"SQLite rejected a value because it violates a CHECK constraint.",
  hint:"Export this operation's diagnostics with the steps needed to reproduce it.",
  operationId:"op-example", runId:"run-example", jobId:"job-example",
  details:{workId:"RJ01760323",errorDetails:{databaseCode:"275",databaseKind:"CheckViolation"}},
};
describe("readable unified logs", () => {
  it("searches work IDs and nested error codes and preserves a complete copyable entry", () => {
    expect(logMatches(event," rj01760323 ")).toBe(true);
    expect(logMatches(event,"275")).toBe(true);
    expect(logMatches(event,"加入下載", "zh-TW")).toBe(true);
    expect(logMatches(event,"拒絕寫入", "zh-TW")).toBe(true);
    expect(logMatches(event,"checkviolation")).toBe(true);
    expect(logMatches(event,"RJ000000")).toBe(false);
    const text=logText(event);
    for(const value of ["09:15:32.135", "Cause:", "Next:", "275", "RJ01760323", "operationId=op-example"]) expect(text).toContain(value);
  });
  it("hides routine reads but keeps read failures and stage transitions", () => {
    expect(technicalEvent({...event,level:"info",outcome:"succeeded",operation:"get_product_detail"})).toBe(true);
    expect(technicalEvent({...event,operation:"get_product_detail"})).toBe(false);
    expect(technicalEvent({...event,level:"info",outcome:"succeeded",operation:"job.phase"})).toBe(false);
    expect(logTitle(event,"en")).toBe("Queue download · Failed");
  });
  it("keeps the detailed cause in the operation while avoiding a duplicate summary row", () => {
    const terminal = {...event, kind:"finished", stage:"result", details:{errorDetails:event.details}};
    const cause = {...event,jobId:null};
    expect(redundantCause(cause,[cause,terminal])).toBe(true);
    expect(redundantCause(event,[event,terminal])).toBe(false); // Individual batch failures remain visible.
    expect(redundantCause(cause,[cause])).toBe(false);
  });
});
