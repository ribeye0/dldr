import { describe, expect, it } from "vitest";
import {
  buildLanguagePriority, defaultLanguagePriority, reorderLanguagePriority,
  moveLanguagePriority,
} from "./name-language-priority";

describe("fixed title language settings", () => {
  it("groups legacy extra languages as fallback and fills only the four supported rows", () => {
    expect(buildLanguagePriority(["ja_JP", "pt_BR", "ko_KR", "und", "xx_XX"]))
      .toEqual(["ja_JP", "zh_TW", "zh_CN", "en_US"]);
    expect(buildLanguagePriority(["ko_KR", "en_US", "ja_JP", "en_US"], "zh-CN"))
      .toEqual(["en_US", "ja_JP", "zh_TW", "zh_CN"]);
    expect(buildLanguagePriority([], "ja")).toEqual(defaultLanguagePriority("ja"));
  });
  it("preserves supported custom order and makes reorder/undo reversible", () => {
    const baseline = buildLanguagePriority(["ja_JP"]);
    const moved = moveLanguagePriority(baseline, "en_US", -1);
    const undone = moveLanguagePriority(moved, "en_US", 1);
    expect(undone).toEqual(baseline);
    expect(moveLanguagePriority(baseline,"pt_BR",-1)).toEqual(baseline);
    expect(moveLanguagePriority(baseline,"ja_JP",-1)).toEqual(baseline);
  });
  it("uses regional Chinese defaults with English before the Other fallback", () => {
    expect(defaultLanguagePriority("zh-TW")).toEqual(["zh_TW", "zh_CN", "ja_JP", "en_US"]);
    expect(defaultLanguagePriority("zh-CN")).toEqual(["zh_CN", "zh_TW", "ja_JP", "en_US"]);
    for (const locale of ["zh-TW", "zh-CN", "ja", "en"] as const)
      expect(defaultLanguagePriority(locale)).not.toContain("ko_KR");
  });
  it("moves a dragged language across multiple positions and ignores unsupported targets", () => {
    const initial = defaultLanguagePriority("zh-TW");
    expect(reorderLanguagePriority(initial, "en_US", "zh_TW")).toEqual(["en_US", "zh_TW", "zh_CN", "ja_JP"]);
    expect(reorderLanguagePriority(initial, "zh_TW", "en_US")).toEqual(["zh_CN", "ja_JP", "en_US", "zh_TW"]);
    expect(reorderLanguagePriority(initial, "ja_JP", "ja_JP")).toEqual(initial);
    expect(reorderLanguagePriority(initial, "ja_JP", "ko_KR")).toEqual(initial);
    expect(initial).toEqual(defaultLanguagePriority("zh-TW"));
  });
});
