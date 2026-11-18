import { describe, expect, it } from "vitest";
import {
  buildLanguagePriority, defaultLanguagePriority, priorityForLocaleChange,
  languagePriorityPatch, moveLanguagePriority,
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
    expect(languagePriorityPatch(baseline, baseline)).toEqual({});
    const moved = moveLanguagePriority(baseline, "en_US", -1);
    expect(languagePriorityPatch(moved, baseline)).toEqual({languagePriority:moved});
    const undone = moveLanguagePriority(moved, "en_US", 1);
    expect(languagePriorityPatch(undone, baseline)).toEqual({});
    expect(moveLanguagePriority(baseline,"pt_BR",-1)).toEqual(baseline);
    expect(moveLanguagePriority(baseline,"ja_JP",-1)).toEqual(baseline);
  });
  it("uses regional Chinese defaults with English before the Other fallback", () => {
    expect(defaultLanguagePriority("zh-TW")).toEqual(["zh_TW", "zh_CN", "ja_JP", "en_US"]);
    expect(defaultLanguagePriority("zh-CN")).toEqual(["zh_CN", "zh_TW", "ja_JP", "en_US"]);
    for (const locale of ["zh-TW", "zh-CN", "ja", "en"] as const)
      expect(defaultLanguagePriority(locale)).not.toContain("ko_KR");
  });
  it("follows a locale change only when the preset has not been reordered", () => {
    const initial=defaultLanguagePriority("zh-TW");
    const simplified=priorityForLocaleChange(initial,"zh-TW","zh-CN");
    expect(simplified).toEqual(defaultLanguagePriority("zh-CN"));
    expect(priorityForLocaleChange(simplified,"zh-CN","zh-TW")).toEqual(initial);
    const custom=moveLanguagePriority(initial,"ja_JP",-1);
    expect(priorityForLocaleChange(custom,"zh-TW","zh-CN")).toEqual(custom);
  });
});
