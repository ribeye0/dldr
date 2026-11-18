#!/usr/bin/env python3
"""Verify four-language genre names against public DLsite pages; write only a complete audit."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import sys
import threading
import time

import requests

ROOT = Path(__file__).resolve().parents[2]
LOCALES = {"zh_TW": "zhTw", "zh_CN": "zhCn", "ja_JP": "ja", "en_US": "en"}
STORES = ("maniax", "home", "girls", "bl")


class Page(HTMLParser):
    """Read identities and headings, never counts or keyword-ranking text."""
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.genre_ids = set()
        self.heading = ""
        self.title = ""
        self.active = None
        self.heading_complete = False

    def handle_starttag(self, tag, attrs):
        if tag in ("h1", "title"):
            self.active = tag
        if tag == "a":
            href = dict(attrs).get("href", "")
            match = re.search(r"/genre/(\d+)(?:/|$|\?)", href)
            if match:
                self.genre_ids.add(int(match[1]))

    def handle_endtag(self, tag):
        if tag == self.active:
            self.active = None
            if tag == "h1":
                self.heading_complete = True

    def handle_data(self, data):
        if self.active == "h1":
            self.heading += data
        elif self.active == "title":
            self.title += data


def label(text, locale, search=False):
    text = text.strip()
    if locale == "en_US":
        pattern = r"\[(.+)\] product list" if search else r"(.+) works"
    else:
        suffix = {"ja_JP": "の作品一覧", "zh_TW": "的作品一覽", "zh_CN": "的作品一览"}[locale]
        pattern = rf"「(.+)」{suffix}"
    match = re.fullmatch(pattern, text)
    return match[1].strip() if match else None


def merge(catalogue, verified):
    previous = {row["id"]: row for row in catalogue}
    result = []
    for genre_id, translations in sorted(verified.items()):
        if set(translations) != set(LOCALES):
            raise ValueError(f"Incomplete official translations for genre {genre_id}")
        row = {"id": genre_id, **{LOCALES[loc]: translations[loc] for loc in LOCALES}}
        old = previous.get(genre_id, {})
        aliases = list(old.get("aliases", []))
        for key in LOCALES.values():
            if old.get(key) and old[key] != row[key] and old[key] not in aliases:
                aliases.append(old[key])
        aliases = [name for name in dict.fromkeys(aliases) if name not in [row[k] for k in LOCALES.values()]]
        if aliases:
            row["aliases"] = aliases
        result.append(row)
    if set(previous) - set(verified):
        raise ValueError("Audit omitted existing genres; refusing to drop cached identities")
    return result


def audit(catalogue, cache=None):
    state = threading.local()

    def get(url, heading_only=False):
        if not hasattr(state, "session"):
            state.session = requests.Session()
        time.sleep(.15)
        with state.session.get(url, stream=True, timeout=30) as response:
            response.raise_for_status()
            page = Page()
            content = bytearray()
            for chunk in response.iter_content(8192):
                content.extend(chunk)
                # Reparse complete UTF-8 text so a split multibyte character is preserved.
                page = Page()
                page.feed(content.decode("utf-8", "replace"))
                if heading_only and page.heading_complete:
                    break
                if len(content) > 2 * 1024 * 1024:
                    raise ValueError(f"Unexpected page size: {url}")
            return page, content.decode("utf-8")

    indices = {}
    for store in STORES:
        for locale in LOCALES:
            path = cache / f"index-{store}-{locale}.html" if cache else None
            if path and path.exists() and time.time() - path.stat().st_mtime < 3600:
                page = Page(); page.feed(path.read_text())
            else:
                page, text = get(f"https://www.dlsite.com/{store}/genre/list/?locale={locale}")
                if path:
                    path.write_text(text)
            expected = {"zh_TW": "分類一覽", "zh_CN": "分类一览", "ja_JP": "ジャンル一覧", "en_US": "Genre list"}[locale]
            if page.heading.strip() != expected or len(page.genre_ids) < 50:
                raise ValueError(f"Invalid official genre index: {store}/{locale}")
            indices[store, locale] = page.genre_ids
    ids = {row["id"] for row in catalogue}.union(*indices.values())
    locations = {}
    for genre_id in sorted(ids):
        locations[genre_id] = next((s for s in STORES if genre_id in indices[s, "ja_JP"]),
                                  next((s for s in STORES if any(genre_id in indices[s, l] for l in LOCALES)), "maniax"))

    def verify(task):
        genre_id, locale = task
        store = locations[genre_id]
        url = f"https://www.dlsite.com/{store}/works/genre/=/genre/{genre_id:03d}/?locale={locale}"
        search_url = f"https://www.dlsite.com/{store}/fsr/=/genre/{genre_id:03d}/?locale={locale}"
        path = cache / f"checked-{genre_id}-{locale}.json" if cache else None
        if path and path.exists() and time.time() - path.stat().st_mtime < 3600:
            old = json.loads(path.read_text())
            if (old.get("id") == genre_id and old.get("locale") == locale
                    and old.get("url") in (url, search_url) and old.get("name")
                    and label(old.get("heading", ""), locale, old["url"] == search_url) == old["name"]):
                return genre_id, locale, old["name"]
        try:
            page, _ = get(url, heading_only=True)
            heading = page.heading.strip()
            name = label(heading, locale)
        except requests.HTTPError as error:
            if error.response.status_code != 404:
                raise
            # Retired index entries may remain available through official search.
            url = search_url
            page, _ = get(url, heading_only=True)
            heading = page.title.split(" | DLsite", 1)[0].strip()
            name = label(heading, locale, search=True)
        if not name:
            raise ValueError(f"No verified official genre heading: {url}")
        if path:
            path.write_text(json.dumps({"id": genre_id, "locale": locale, "url": url,
                                        "heading": heading, "name": name}, ensure_ascii=False, indent=2) + "\n")
        return genre_id, locale, name

    verified = {}
    failures = []
    tasks = [(genre_id, loc) for genre_id in sorted(ids) for loc in LOCALES]
    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = {pool.submit(verify, task): task for task in tasks}
        for count, future in enumerate(as_completed(futures), 1):
            try:
                genre_id, locale, name = future.result()
                verified.setdefault(genre_id, {})[locale] = name
            except (requests.RequestException, ValueError) as error:
                failures.append((futures[future], str(error)))
            if count % 100 == 0:
                print(f"Verified {count}/{len(tasks)} pages; failures: {len(failures)}", flush=True)
    if failures:
        raise ValueError(f"Official verification incomplete; catalogue unchanged: {failures}")
    return merge(catalogue, verified)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="Replace the catalogue after every locale passes")
    parser.add_argument("--cache-dir", type=Path, help="Resume the same audit using a one-hour public-page cache")
    args = parser.parse_args()
    if args.cache_dir:
        args.cache_dir.mkdir(parents=True, exist_ok=True)
    path = ROOT / "resources/genres.json"
    previous = json.loads(path.read_text())
    updated = audit(previous, args.cache_dir)
    old = {row["id"]: row for row in previous}
    changes = 0
    for row in updated:
        for key in LOCALES.values():
            before = old.get(row["id"], {}).get(key)
            if before != row[key]:
                changes += 1
                print(f"{row['id']} {key}: {before!r} -> {row[key]!r}")
    print(f"{len(updated)} genres, {len(updated) * len(LOCALES)} verified names, {changes} changes")
    if args.write:
        path.write_text(json.dumps(updated, ensure_ascii=False, indent=2) + "\n")
    elif changes:
        print("Run with --write to apply these verified labels.")
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (requests.RequestException, ValueError) as error:
        sys.exit(str(error))
