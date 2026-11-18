import { describe, it, expect } from "vitest";
import { get } from "svelte/store";
import { locale, locales, messages, normalizeLocale, translator, t } from "./index";
import { genreName, localizeGenre } from "./genres";
import catalogue from "$data/genres.json";
import { jobLabel } from "$lib/utils/jobs";

describe("interface languages", () => {
  it("provides the same message keys in all four independently written catalogues", () => {
    for (const language of locales) {
      expect(Object.keys(messages[language]).sort()).toEqual(Object.keys(messages.en).sort());
      expect(Object.values(messages[language]).every(value => value.trim())).toBe(true);
      for (const [key, value] of Object.entries(messages[language])) {
        // Translators may omit English plural suffix slots, but cannot invent parameters.
        for (const slot of value.match(/\{\d+\}/g) ?? []) expect(key).toContain(slot);
      }
    }
    expect(translator("zh-TW")("Library folder")).toBe("作品庫資料夾");
    expect(translator("zh-CN")("Library folder")).toBe("作品库文件夹");
    expect(normalizeLocale("fr")).toBe("zh-TW");
  });
  it("updates labels and genres together without interpolating parameter contents twice", () => {
    locale.set("ja");
    expect(get(t)("Settings")).toBe("設定");
    expect(get(genreName)("強勢女")).toBe("女性優位");
    locale.set("en");
    expect(get(t)("Settings")).toBe("Settings");
    expect(get(genreName)("強勢女")).toBe("Femdom");
    expect(get(t)("Copy {0}", "{1}<script>" )).toBe("Copy {1}<script>");
    locale.set("zh-TW");
  });
  it("localizes download labels and language names independently from title priority", () => {
    const job = {status: "running", phase: "unpacking", kind: "workDownload"} as Parameters<typeof jobLabel>[0];
    expect(jobLabel(job, "ja")).toBe("展開中");
    expect(jobLabel(job, "zh-CN")).toBe("解压中");
  });
});

describe("localized genre identity", () => {
  it("has a label for every bundled numeric ID in each language", () => {
    for (const genre of catalogue) {
      for (const language of locales) {
        const key = { "zh-TW": "zhTw", "zh-CN": "zhCn", ja: "ja", en: "en" }[language] as "zhTw" | "zhCn" | "ja" | "en";
        expect(localizeGenre(genre.zhTw, language, genre.id)).toBe(genre[key]);
      }
      expect(genre.zhCn).toBeTruthy(); expect(genre.en).toBeTruthy(); expect(genre.ja).toBeTruthy();
    }
  });
  it("keeps all NTR perspectives and homonymous genres separate by official ID", () => {
    const labels = [
      { id: 48, zhTw: "NTR", zhCn: "被NTR(苦主视角)", ja: "寝取られ", en: "Cuckoldry (Netorare)" },
      { id: 302, zhTw: "寢取/NTR", zhCn: "NTR(黄毛视角)", ja: "寝取り", en: "Cuckoldry (Netori)" },
      { id: 529, zhTw: "故意被NTR/綠奴", zhCn: "故意被NTR/绿奴", ja: "寝取らせ", en: "Cuckoldry (Netorase)" },
    ];
    for (const genre of labels) {
      expect(localizeGenre("old cached name", "zh-TW", genre.id)).toBe(genre.zhTw);
      expect(localizeGenre("old cached name", "zh-CN", genre.id)).toBe(genre.zhCn);
      expect(localizeGenre("old cached name", "ja", genre.id)).toBe(genre.ja);
      expect(localizeGenre("old cached name", "en", genre.id)).toBe(genre.en);
    }
    expect(localizeGenre("NTR", "zh-CN")).toBe("被NTR(苦主视角)");
    expect(localizeGenre("被寝取/NTR", "zh-CN")).toBe("被NTR(苦主视角)");
    expect(localizeGenre("短髮", "ja", 166)).toBe("ショートカット");
    expect(localizeGenre("短髮", "ja", 478)).toBe("短髪");
    expect(localizeGenre("短髮", "en", 478)).toBe("Short Hair (Male)");
  });
  it("uses regional taxonomy terms rather than script conversion", () => {
    for (const alias of ["強勢女", "女性主导", "女性主導", "女性優位", "Femdom"]) {
      expect(localizeGenre(alias, "zh-TW")).toBe("強勢女");
      expect(localizeGenre(alias, "zh-CN")).toBe("女性主导");
      expect(localizeGenre(alias, "ja")).toBe("女性優位");
      expect(localizeGenre(alias, "en")).toBe("Femdom");
    }
    expect(localizeGenre("雜~魚♡/美式咖哩", "zh-CN")).toBe("雌小鬼");
    expect(localizeGenre("耳舐め", "en")).toBe("Ear Licking");
  });
  it("removes spurious genre prefixes while resolving legacy labels and real numeric names", () => {
    for (const alias of ["6 萝", "6萝", "6 つるぺた", "6 Petite", "萝"]) {
      expect(localizeGenre(alias, "zh-CN")).toBe("萝");
      expect(localizeGenre(alias, "ja")).toBe("つるぺた");
      expect(localizeGenre(alias, "en")).toBe("Petite");
    }
    for (const alias of ["7 反向奸", "7反向奸", "7 逆レ", "7 Reverse R", "反向奸"]) {
      expect(localizeGenre(alias, "zh-CN")).toBe("反向奸");
      expect(localizeGenre(alias, "ja")).toBe("逆レ");
      expect(localizeGenre(alias, "en")).toBe("Reverse R");
    }
    expect(localizeGenre("3D作品", "zh-CN")).toBe("3D作品");
    expect(localizeGenre("3D作品", "en")).toBe("3D Works");
  });
  it("preserves unrecognized metadata and gives unknown IDs an explicit label", () => {
    expect(localizeGenre("分類 #999999", "en")).toBe("Genre #999999");
    expect(localizeGenre("future genre", "zh-CN")).toBe("future genre");
  });
});
