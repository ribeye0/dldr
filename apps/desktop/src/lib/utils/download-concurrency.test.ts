import { expect, it } from "vitest";
import { downloadConcurrency } from "./download-concurrency";

it("restores only supported whole-number concurrency preferences", () => {
  expect(downloadConcurrency("1")).toBe(1);
  expect(downloadConcurrency("4")).toBe(4);
  for (const value of [null, undefined, "", "junk", "0", "5", "2.5", NaN, Infinity, -1]) {
    expect(downloadConcurrency(value)).toBe(2);
  }
});
