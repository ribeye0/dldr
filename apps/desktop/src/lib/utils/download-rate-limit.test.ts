import { expect, it } from "vitest";
import { downloadRateBytes } from "./download-rate-limit";

it("converts MiB/s to whole bytes and preserves unlimited", () => {
  expect(downloadRateBytes(0)).toBeNull();
  expect(downloadRateBytes(0.5)).toBe(524288);
  expect(downloadRateBytes(0.0009765625)).toBe(1024);
  expect(downloadRateBytes(1024)).toBe(1073741824);
});
it("rejects empty, nonfinite and out-of-range input", () => {
  for (const value of [undefined, NaN, Infinity, -1, 1025, 0.0001]) {
    expect(() => downloadRateBytes(value)).toThrow();
  }
});
