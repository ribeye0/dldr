export type SearchKind = "text" | "circle" | "va" | "tag" | "-tag";
export type SearchTerm = { kind: SearchKind; value: string; start: number; end: number };
export type ParsedSearch = { terms: SearchTerm[]; error: string | null };

export const searchLabels: Record<SearchKind, string> = {
  text: "關鍵字", circle: "社團", va: "聲優", tag: "分類", "-tag": "排除分類",
};

// Presentation/inline validation only. Rust applies the same grammar before
// database counts, pagination, facets and bulk candidate selection.
export function parseProductSearch(input: string): ParsedSearch {
  const terms: SearchTerm[] = [];
  const fail = (error: string): ParsedSearch => ({ terms, error });
  if (Array.from(input).length > 4096) return fail("搜尋內容最多 4096 字。");
  const trimmed = input.trim();
  if (/^(not owned|local only)$/i.test(trimmed) || /^(?:分類|分类|ジャンル|Genre) #-?\d+$/.test(trimmed)) {
    const start = input.indexOf(trimmed);
    return { terms: [{kind:"text", value:trimmed, start, end:start+trimmed.length}], error:null };
  }
  let offset = 0;
  while (offset < input.length) {
    if (/\s/.test(input[offset])) { offset++; continue; }
    if (terms.length >= 64) return fail("搜尋條件最多 64 個。");
    const start = offset;
    if (input[offset] === '"') {
      const end = input.indexOf('"', offset + 1);
      if (end < 0) return fail("文字片語缺少結尾雙引號。");
      const value = input.slice(offset + 1, end).trim();
      if (!value) return fail("文字片語不能為空。");
      terms.push({kind:"text", value, start, end:end+1}); offset=end+1; continue;
    }
    if (input[offset] === "$" && /^[^$\s]*:/.test(input.slice(offset+1))) {
      const end = input.indexOf("$", offset+1);
      if (end < 0) return fail("精準條件缺少結尾 $，例如 $tag:NTR$。");
      const colon = input.indexOf(":",offset+1);
      const kind = input.slice(offset+1,colon).toLowerCase();
      const value = input.slice(colon+1,end).trim();
      if (!value) return fail("精準條件的名稱不能為空。");
      if (!["circle","va","tag","-tag"].includes(kind)) return fail("不支援此搜尋指令；可用 circle、va、tag、-tag。");
      terms.push({kind:kind as SearchKind,value,start,end:end+1});offset=end+1;continue;
    }
    while (offset < input.length && !/\s/.test(input[offset])) offset++;
    terms.push({kind:"text",value:input.slice(start,offset),start,end:offset});
  }
  return {terms,error:null};
}

export function searchToken(kind: Exclude<SearchKind,"text">, value: string) {
  return `$${kind}:${value}$`;
}

export function appendSearchToken(input: string, kind: Exclude<SearchKind,"text">, value: string) {
  const parsed = parseProductSearch(input);
  if (parsed.terms.some(term => term.kind === kind && term.value.toLowerCase() === value.toLowerCase())) return input;
  return [input.trim(),searchToken(kind,value)].filter(Boolean).join(" ");
}

export function removeSearchTerm(input: string, term: SearchTerm) {
  return `${input.slice(0,term.start).trimEnd()} ${input.slice(term.end).trimStart()}`.trim();
}

type ExactSearchKind = Exclude<SearchKind, "text">;
export type SearchValue = { kind: Exclude<ExactSearchKind, "-tag">; name: string; aliases?: string[] };
export type SearchCompletion = {
  id: string; label: string; value: string; caret: number;
  kind: ExactSearchKind; command: boolean;
};
const exactKinds: ExactSearchKind[] = ["circle", "va", "tag", "-tag"];

// Complete only the token under the caret. Quoted text and ordinary keywords
// retain their literal meaning; names may contain spaces and tokens may touch.
export function productSearchCompletions(input: string, caret: number, choices: SearchValue[]): SearchCompletion[] {
  if (input.length > 4096 || caret < 0 || caret > input.length) return [];
  let start = 0;
  while (start < caret) {
    if (/\s/.test(input[start])) { start++; continue; }
    if (input[start] === '"') {
      const end = input.indexOf('"', start + 1);
      if (end < 0 || caret <= end) return [];
      start = end + 1; continue;
    }
    const field = input.slice(start).match(/^\$([^$\s:]*):/);
    if (field) {
      const close = input.indexOf("$", start + 1);
      const end = close < 0 ? input.length : close;
      if (caret <= end) {
        const kind = field[1].toLowerCase() as ExactSearchKind;
        if (!exactKinds.includes(kind) || caret < start + field[0].length) return [];
        const query = input.slice(start + field[0].length, caret).trim();
        const normalized = query.toLocaleLowerCase();
        const source = kind === "-tag" ? "tag" : kind;
        const names = [...new Set(choices.filter(choice => choice.kind === source &&
          [choice.name, ...(choice.aliases ?? [])].some(name => name.toLocaleLowerCase().includes(normalized)))
          .map(choice => choice.name.trim()).filter(name => name && !/[\n\r$]/.test(name)))];
        names.sort((a, b) => Number(b.toLocaleLowerCase().startsWith(normalized)) - Number(a.toLocaleLowerCase().startsWith(normalized)));
        // An uncached name is still valid syntax; never require a suggestion.
        if (!names.length && query && !/[\n\r$]/.test(query)) names.push(query);
        return names.slice(0, 8).map(name => {
          const token = searchToken(kind, name);
          const suffix = input.slice(close < 0 ? end : end + 1);
          const space = suffix && !/^\s|^\$/.test(suffix) ? " " : "";
          return { id: token, label: name, kind, command: false,
            value: input.slice(0, start) + token + space + suffix,
            caret: start + token.length + space.length };
        });
      }
      if (close < 0) return [];
      start = close + 1; continue;
    }
    let end = start + 1;
    while (end < input.length && !/\s|\$/.test(input[end])) end++;
    if (input[start] === "$" && caret <= end) {
      const prefix = input.slice(start + 1, caret).toLowerCase();
      if (!/^-?[a-z]*$/.test(prefix)) return [];
      return exactKinds.filter(kind => kind.startsWith(prefix)).map(kind => {
        const token = `$${kind}:`;
        return { id: kind, label: token, kind, command: true,
          value: input.slice(0, start) + token + input.slice(end), caret: start + token.length };
      });
    }
    // A dollar sign inside an ordinary keyword is not a new condition.
    while (end < input.length && !/\s/.test(input[end])) end++;
    start = end;
  }
  return [];
}
