import type {
  Product,
  ProductDetail,
  ProductFilterFacets,
  ProductTextValue,
} from "$lib/model/types";
import { productIsLocalOnly, productType } from "$lib/utils/products";
import { parseProductSearch } from "$lib/utils/product-search";
import genreCatalogue from "$data/genres.json";

// Local Storybook projections only. Production queries remain owned by Rust.
export type PreviewProduct = Product & {
  makerKey?: string;
  titleVariants?: ProductTextValue[];
  makerNames?: ProductTextValue[];
};
export type PreviewFilters = {
  genreNames?: string[]; excludedGenreNames?: string[]; voiceNames?: string[]; excludedVoiceNames?: string[];
  downloadedOnly?: boolean;
  sort: string;
  accounts: string[];
  sources: string[];
  ages: string[];
  types: string[];
  makers: string[];
};
export function emptyFilters(): PreviewFilters {
  return {
    sort: "latestPurchaseDesc",
    accounts: [],
    sources: [],
    ages: [],
    types: [],
    makers: [],
  };
}
export function toggle(values: string[], value: string) {
  return values.includes(value)
    ? values.filter((x) => x !== value)
    : [...values, value];
}
export function filterCount(filters: PreviewFilters) {
  return (
    filters.accounts.length +
    filters.sources.length +
    filters.ages.length +
    filters.types.length +
    filters.makers.length
  );
}
export function queryProducts(
  products: PreviewProduct[],
  search: string,
  filters: PreviewFilters,
) {
  const parsed = parseProductSearch(search);
  if (parsed.error) throw new Error(parsed.error);
  const lower = (value: string) => value.trim().toLowerCase();
  const genreNames = (genre: (typeof genreCatalogue)[number]) => [genre.zhTw,genre.zhCn,genre.en,genre.ja,...("aliases" in genre ? genre.aliases ?? [] : [])].filter((name): name is string => typeof name === "string");
  const canonicalGenre = (name: string) => genreCatalogue.find(genre => genreNames(genre).some(alias => lower(alias) === lower(name)))?.zhTw ?? name;
  const any = (selected: string[], value: string) =>
    !selected.length || selected.includes(value);
  return products
    .filter((p) => {
      const local = productIsLocalOnly(p);
      const haystack = [
        p.title,
        p.makerName,
        ...(p.titleVariants ?? []).map((value) => value.value),
        ...(p.makerNames ?? []).map((value) => value.value),
        p.workId,
        ...p.creditGroups.flatMap((g) => g.names),
        ...(p.tags ?? []).map((t) => t.name),
        ...p.owners.map((o) => o.label),
        local ? "local only not owned" : "owned",
      ]
        .join(" ")
        .toLowerCase();
      const tone = productType(p).tone;
      return (
        (filters.genreNames ?? []).every(name => p.tags?.some(tag => tag.class === "genre" && tag.name === name)) &&
        !(filters.excludedGenreNames ?? []).some(name => p.tags?.some(tag => tag.class === "genre" && tag.name === name)) &&
        (!(filters.voiceNames ?? []).length || (filters.voiceNames ?? []).some(name => p.creditGroups.some(g => g.kind === "voice" && g.names.includes(name)))) &&
        !(filters.excludedVoiceNames ?? []).some(name => p.creditGroups.some(g => g.kind === "voice" && g.names.includes(name))) &&
        (!filters.downloadedOnly || (p.download.status === "downloaded" && !!p.download.localPath?.trim())) &&
        parsed.terms.every(term => {
          const name=lower(term.value);
          const genres=(p.tags ?? []).filter(tag=>tag.class === "genre").map(tag=>tag.name);
          switch(term.kind) {
            case "text": return haystack.includes(name) || genreCatalogue.some(genre=>genreNames(genre).some(alias=>lower(alias).includes(name)) && genres.includes(genre.zhTw));
            case "circle": return [p.makerName,...(p.makerNames ?? []).map(n=>n.value)].some(n=>typeof n === "string" && lower(n)===name);
            case "va": return p.creditGroups.some(group=>group.kind === "voice" && group.names.some(n=>lower(n)===name));
            case "tag": return genres.some(n=>lower(canonicalGenre(n))===lower(canonicalGenre(term.value)));
            case "-tag": return !genres.some(n=>lower(canonicalGenre(n))===lower(canonicalGenre(term.value)));
          }
        }) &&
        (!filters.accounts.length ||
          p.owners.some((o) => filters.accounts.includes(o.accountId))) &&
        any(filters.sources, local ? "localOnly" : "owned") &&
        any(filters.ages, p.ageCategory ?? "") &&
        any(filters.types, tone === "voice-comic" ? "image" : tone) &&
        any(
          filters.makers,
          p.makerKey ?? (p.makerName ? `name:${p.makerName}` : ""),
        )
      );
    })
    .sort((a, b) => {
      const dateKey =
        filters.sort === "publishedAtDesc"
          ? "publishedAt"
          : "latestPurchasedAt";
      const byDate =
        filters.sort === "titleAsc"
          ? 0
          : (b[dateKey] ?? "").localeCompare(a[dateKey] ?? "");
      return (
        byDate ||
        a.title.localeCompare(b.title) ||
        a.workId.localeCompare(b.workId)
      );
    });
}
export function facetsFor(
  products: PreviewProduct[],
  search = "",
  filters = emptyFilters(),
): ProductFilterFacets {
  const count = (values: string[]) =>
    [...new Set(values)]
      .sort()
      .map((name) => ({
        name,
        count: values.filter((x) => x === name).length,
      }))
      .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  // Match the production facet boundary: omit only the facet's own selection.
  const makerProducts = queryProducts(products, search, {
    ...filters,
    makers: [],
  });
  const makers = new Map<
    string,
    { key: string; name: string; count: number }
  >();
  for (const product of makerProducts) {
    if (!product.makerName) continue;
    const key = product.makerKey ?? `name:${product.makerName}`;
    const facet = makers.get(key) ?? { key, name: product.makerName, count: 0 };
    facet.count += 1;
    makers.set(key, facet);
  }
  const metadataProducts = queryProducts(products,search,{...filters,genreNames:[],excludedGenreNames:[],voiceNames:[],excludedVoiceNames:[]});
  return {
    genres: count(metadataProducts.flatMap(p => [...new Set((p.tags ?? []).filter(t => t.class === "genre").map(t => t.name))])),
    voices: count(metadataProducts.flatMap(p => [...new Set(p.creditGroups.filter(g => g.kind === "voice").flatMap(g => g.names))])),
    makers: [...makers.values()].sort(
      (a, b) => b.count - a.count || a.name.localeCompare(b.name),
    ),
  };
}
export function makeDetail(product: PreviewProduct): ProductDetail {
  const local = productIsLocalOnly(product);
  return {
    ...product,
    titleVariants:
      product.titleVariants ??
      (local
        ? []
        : [
            { language: "en_US", value: product.title },
            { language: "ja_JP", value: "静かな夜の物語 — サンプル作品" },
          ]),
    makerId: local ? null : "RG01234567",
    makerNames:
      product.makerNames ??
      (product.makerName
        ? [
            { language: "en_US", value: product.makerName },
            { language: "ja_JP", value: "北窓スタジオ" },
          ]
        : []),
    contentSizeBytes: local ? null : product.contentSizeBytes ?? null,
    registeredAt: local ? null : "2026-07-20T00:00:00Z",
    lastDetailSyncAt: "2026-09-28T05:00:00Z",
    tags: product.tags ?? (local
      ? []
      : [
          { class: "genre", name: "Slice of life" },
          { class: "genre", name: "Relaxing" },
        ]),
  };
}
export function parseTags(value: string, existing: string[]): string[] {
  const names = new Map<string, string>();
  for (const part of [...existing, ...value.split(/[,\n]/)]) {
    const name = part.trim().replace(/\s+/g, " ");
    if ([...name].length > 64)
      throw new Error("Each tag must be 64 characters or fewer.");
    if (name && !names.has(name.toLowerCase()))
      names.set(name.toLowerCase(), name);
  }
  return [...names.values()].sort((a, b) => a.localeCompare(b));
}
