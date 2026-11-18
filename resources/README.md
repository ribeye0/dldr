# DLsite genre catalogue

`genres.json` maps stable numeric genre IDs to DLsite's official Traditional
Chinese, Simplified Chinese, Japanese and English names. Display uses the genre
ID, not an inferred Simplified/Traditional conversion or a name in another
language. Existing cached purchases do not need to be synced again.

## Official sources

Verified **2026-11-18**: **405 genres / 1,620 localized names**, including retired
IDs still present in purchased libraries. Genre indexes discover IDs; the
individual genre page's heading supplies the current label. Some index labels
are older than the individual pages, so index anchor text is not authoritative
for display.

The verification covers the public genre indexes in each of these stores, in
all four locales (`zh_TW`, `zh_CN`, `ja_JP`, `en_US`):

- [Doujin/R18](https://www.dlsite.com/maniax/genre/list/?locale=zh_CN)
- [All ages](https://www.dlsite.com/home/genre/list/?locale=zh_TW)
- [Otome/TL](https://www.dlsite.com/girls/genre/list/?locale=ja_JP)
- [BL](https://www.dlsite.com/bl/genre/list/?locale=en_US)

Canonical headings use `/{store}/works/genre/=/genre/{ID:03d}/?locale={locale}`.
For example, [genre 048 in Simplified Chinese](https://www.dlsite.com/maniax/works/genre/=/genre/048/?locale=zh_CN)
is `被NTR(苦主视角)`, while [302](https://www.dlsite.com/maniax/works/genre/=/genre/302/?locale=zh_CN)
is `NTR(黄毛视角)` and [529](https://www.dlsite.com/maniax/works/genre/=/genre/529/?locale=zh_CN)
is `故意被NTR/绿奴`. These are distinct IDs even when an old translation used
`NTR` for more than one of them.

Retired ID 454 is verified using the title of its
[official search page](https://www.dlsite.com/maniax/fsr/=/genre/454/?locale=en_US).
IDs 516, 517 and 518 currently remain `TRPGシナリオ`, `TRPGリプレイ集` and
`TRPGルールブック` on all four official locale pages. Keep those official labels;
previous local translations are search aliases only. Unknown future IDs display
an explicit localized `Genre #ID` placeholder.

## Updating

Install Python `requests`, then run from the repository root:

```sh
python3 tools/dev/sync-genres.py
python3 tools/dev/sync-genres.py --write
python3 tools/dev/test-sync-genres.py
cargo test -p dldr-storage
pnpm test
```

The first command checks current official pages and reports differences without
changing files; a nonzero exit means differences or incomplete verification.
`--write` applies changes only after every ID and locale has been verified. It
also discovers new IDs and retains previous labels as search aliases. It never
fills a missing language with a guessed translation or deletes old IDs. Use
`--cache-dir /tmp/dldr-genres` on both commands to reuse public responses for up
to one hour when reviewing the same audit. Omit it for a fresh verification.

Review changes and update the verification date/count above after a successful
run. Do not commit response caches, reports or fetched product pages. Rebuild and
install DLDR to update the daily application; changing the source catalogue alone
does not update installed binaries.

- Preserve official spelling, punctuation and capitalization. Each locale has
  independent terminology; do not use automatic script conversion.
- Do not extract whole popular-keyword links: counts or `span.ranking-number`
  contaminate names. Real names such as `3D作品` must retain their digits.
- Current official names take precedence when an old alias conflicts with a
  different genre. Numeric metadata takes precedence over old name-only tags.
- `aliases` are additional search vocabulary, not display labels. Preserve older
  cache names, including the historical prefixed forms for IDs 115 and 207.
- Preserve numeric IDs through storage, native IPC, cards, details, facets and
  CLI output. Duplicate names, such as the two Chinese `短髮` labels, do not imply
  identical IDs or identical Japanese/English labels.
