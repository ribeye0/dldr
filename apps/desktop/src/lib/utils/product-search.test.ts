import { describe, it, expect } from "vitest";
import { appendSearchToken, parseProductSearch, productSearchCompletions, removeSearchTerm, searchToken, type SearchValue } from "./product-search";

describe("product search syntax", () => {
  it("keeps plain keywords and exact conditions separate, including adjacent tokens", () => {
    const parsed = parseProductSearch('NTR 青梅竹馬 $circle:バイコーンの森$$va:小花衣こっこ$ $tag:女性主导$ $-tag:舔耳$');
    expect(parsed.error).toBeNull();
    expect(parsed.terms.map(({kind,value})=>({kind,value}))).toEqual([
      {kind:"text",value:"NTR"},{kind:"text",value:"青梅竹馬"},
      {kind:"circle",value:"バイコーンの森"},{kind:"va",value:"小花衣こっこ"},
      {kind:"tag",value:"女性主导"},{kind:"-tag",value:"舔耳"},
    ]);
  });
  it("accepts quoted phrases, field names with spaces and literal currency", () => {
    expect(parseProductSearch('"A Long Evening" $VA: Actor Name $ $5').terms.map(t=>t.value)).toEqual(['A Long Evening','Actor Name','$5']);
    expect(parseProductSearch("not owned").terms).toHaveLength(1);
    expect(parseProductSearch("分類 #999999").terms).toHaveLength(1);
  });
  it.each(['$tag:$','$tag:NTR','$unknown:value$','$tagw:NTR$','$duration:60$','"unfinished','""'])("rejects invalid syntax without silently dropping conditions: %s", input => {
    expect(parseProductSearch(input).error).not.toBeNull();
  });
  it("bounds work for large inputs and many conditions", () => {
    expect(parseProductSearch("字".repeat(4097)).error).not.toBeNull();
    expect(parseProductSearch("a ".repeat(65)).error).not.toBeNull();
    expect(parseProductSearch("")).toEqual({terms:[],error:null});
  });
  it("removes only the selected condition, preserving spaces inside other names", () => {
    const input='NTR $circle:Circle Name$ $va:Actor Name$';
    const term=parseProductSearch(input).terms[1];
    expect(removeSearchTerm(input,term)).toBe('NTR $va:Actor Name$');
    expect(searchToken("tag","低語")).toBe('$tag:低語$');
  });
  it("adds clicked metadata without discarding text or other conditions", () => {
    expect(appendSearchToken("雨夜 $tag:低語$","va","小花衣こっこ")).toBe("雨夜 $tag:低語$ $va:小花衣こっこ$");
    expect(appendSearchToken("$tag:NTR$","tag","ntr")).toBe("$tag:NTR$");
  });
});

describe("product search completion", () => {
  const choices: SearchValue[] = [
    { kind: "circle", name: "Circle Name" },
    { kind: "va", name: "小花衣こっこ" },
    { kind: "va", name: "Actor Name" },
    { kind: "tag", name: "低語", aliases: ["ささやき", "Whispering"] },
    { kind: "tag", name: "舔耳" },
  ];
  const complete = (input: string, caret = input.length) => productSearchCompletions(input, caret, choices);

  it("offers only supported commands after $, with no default or plain-text recommendations", () => {
    expect(complete("$").map(item => item.label)).toEqual(["$circle:", "$va:", "$tag:", "$-tag:"]);
    expect(complete("雨夜 $-t").map(item => item.value)).toEqual(["雨夜 $-tag:"]);
    for (const input of ["", "NTR", "作品名", "$5", "$duration:", '"$ta', "price$ta"]) expect(complete(input)).toEqual([]);
  });
  it("completes localized values through aliases and closes the condition", () => {
    expect(complete("雨夜 $tag:ささ")[0].value).toBe("雨夜 $tag:低語$");
    expect(complete("$-tag:WHIS")[0].value).toBe("$-tag:低語$");
    expect(complete("$VA:小花")[0].value).toBe("$va:小花衣こっこ$");
    expect(complete("$circle:Circle N")[0].value).toBe("$circle:Circle Name$");
  });
  it("replaces only the token under the caret, preserving later conditions", () => {
    const input = "NTR $va:Act$ $tag:低語$";
    const item = complete(input, input.indexOf("Act") + 3)[0];
    expect(item.value).toBe("NTR $va:Actor Name$ $tag:低語$");
    expect(item.caret).toBe("NTR $va:Actor Name$".length);
    const adjacent = "$tag:低語$$va:Act$";
    expect(complete(adjacent, adjacent.length - 1)[0].value).toBe("$tag:低語$$va:Actor Name$");
    expect(complete('"$va:Act"')).toEqual([]);
    expect(complete("$va:Actor Name$")).toEqual([]);
  });
  it("allows uncached names, deduplicates matches and bounds the dropdown", () => {
    expect(complete("$tag:新分類")[0].value).toBe("$tag:新分類$");
    const many = Array.from({ length: 20 }, (_, i) => ({ kind: "va" as const, name: `Actor ${i}` }));
    expect(productSearchCompletions("$va:", 4, [...many, ...many])).toHaveLength(8);
    expect(productSearchCompletions("$va:", 4, [{ kind: "va", name: "bad$name" }])).toEqual([]);
  });
});
