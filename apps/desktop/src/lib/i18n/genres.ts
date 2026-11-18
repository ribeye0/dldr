import { derived } from "svelte/store";
import catalogue from "$data/genres.json";
import { locale, type UiLocale } from "./index";

type Genre = typeof catalogue[number];
const byId = new Map(catalogue.map(genre => [genre.id, genre]));
const byName = new Map<string, Genre>();
// Storage's canonical names take precedence over another locale or old alias.
for (const names of [
  (genre: Genre) => [genre.zhTw],
  (genre: Genre) => [genre.zhCn, genre.ja, genre.en],
  (genre: Genre) => genre.aliases ?? [],
]) {
  for (const genre of catalogue) {
    for (const name of names(genre)) {
      if (name && !byName.has(name.toLowerCase())) byName.set(name.toLowerCase(), genre);
    }
  }
}
function resolve(name: string, genreId?: number | null): Genre | undefined {
  return genreId != null ? byId.get(genreId) : byName.get(name.trim().toLowerCase());
}
export function localizeGenre(name: string, language: UiLocale, genreId?: number | null): string {
  const genre = resolve(name, genreId);
  if (genre) return ({ "zh-TW": genre.zhTw, "zh-CN": genre.zhCn, ja: genre.ja, en: genre.en })[language];
  const unknown = genreId ?? name.match(/^(?:分類|分类|ジャンル|Genre) #(\d+)$/)?.[1];
  if (unknown != null) return `${{ "zh-TW": "分類", "zh-CN": "分类", ja: "ジャンル", en: "Genre" }[language]} #${unknown}`;
  return name;
}
export const genreName = derived(locale, language => (name: string, genreId?: number | null) => localizeGenre(name, language, genreId));

export function genreSearchNames(name: string, genreId?: number | null): string[] {
  const genre = resolve(name, genreId);
  return genre ? [genre.zhTw, genre.zhCn, genre.ja, genre.en, ...(genre.aliases ?? [])] : [name];
}
