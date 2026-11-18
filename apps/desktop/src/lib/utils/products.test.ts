import { describe, expect, it } from "vitest";
import { productGenreTags, productBadgeTone, productTypeFromCode } from "./products";

describe("cover card tags", () => {
  it("shows only genre tags and removes blank and duplicate labels", () => {
    expect(productGenreTags({
      tags: [{ class: "genre", name: " 低語 " }, { class: "genre", name: "舔耳" }, { class: "genre", name: "低語" }, { class: "genre", name: " " }, { class: "voice_by", name: "Voice A" }],
    }).map(tag => tag.name)).toEqual(["低語", "舔耳"]);
  });
  it("returns no genres when genre metadata is absent", () => {
    expect(productGenreTags({})).toEqual([]);
    expect(productGenreTags({ tags: [] })).toEqual([]);
  });
  it("retains distinct IDs with the same name and deduplicates repeated IDs", () => {
    const tags = productGenreTags({ tags: [
      { class: "genre", name: "短髮", genreId: 166 },
      { class: "genre", name: "短髮", genreId: 478 },
      { class: "genre", name: "old name", genreId: 478 },
    ] });
    expect(tags.map(tag => tag.genreId)).toEqual([166, 478]);
  });
});


describe("work badge categories", () => {
  it("distinguishes comics from illustrations without changing the shared image query group", () => {
    expect(productTypeFromCode("MNG").tone).toBe("image");
    expect(productTypeFromCode("ICG").tone).toBe("image");
    for (const code of ["COM", "MNG", "SCM", "DOH", "VCM", "Voice Comic"]) {
      expect(productBadgeTone(code)).toBe("comic");
    }
    for (const code of ["ICG", "IMT", "illustration", "CG"]) {
      expect(productBadgeTone(code)).toBe("illustration");
    }
    expect(productBadgeTone("SOU")).toBe("audio");
    expect(productBadgeTone("GAM")).toBe("game");
    expect(productBadgeTone("MOV")).toBe("video");
    expect(productBadgeTone("NRE")).toBe("other");
    expect(productBadgeTone(null)).toBe("other");
  });
});
