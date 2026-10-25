# Lava Scans

Lava Scans (<https://lavascans.com>), an Arabic manhwa and manhua translation site, as a manga source plug-in for
Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). A MangaThemesia site with its own cards, title page and chapter
list: the ops, URLs and parsers are the shared [`lib/mangathemesia`](../../lib/mangathemesia) crate, and `src/lib.rs`
holds only the site's selectors.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/lavascans` extension and its `mangathemesia` theme
(`MangaThemesiaAlt`, Apache-2.0). The source id is the one the extension sets: `3209001028102012989`.

Many title slugs carry a number the site changes from time to time (`/manga/2072267132-the-eternal-supreme/`), and the
old address then answers 404. As in the Kotlin (with its "dynamic URL" setting on, the default), titles are stored
without it (`/manga/the-eternal-supreme/`) and found through the site's text list, `/manga/list-mode/`, which maps each
title to its current slug. The map is kept as a preference and fetched again when a title is missing from it or its
saved address answers 404.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `request-headers` | the site as Referer (the images load without it too) |
| `popular` | `GET /manga/?page=N&order=popular` (the `.manga-card-v` cards); a next page when there is a "next" link |
| `latest` | `GET /manga/?page=N&order=update` |
| `search` | `GET /manga/?title=...&page=N` |
| `details` | the title page (`/manga/<current slug>/`, through the map): title, summary, genres, status, cover |
| `chapters` | the same page: the free chapters, newest first, with dates (`2026/10/19`) |
| `pages` | the chapter page: the reader's images |

## Not ported

- **Paid chapters.** They are left out of the list, as with the Kotlin's "hide paid chapters" setting at its default;
  there is no way to buy or read them.
- The settings (dynamic URL, hide paid chapters): both are fixed at the Kotlin's defaults.
- The search filters, the view counter and related titles.
- Chapter addresses keep the slug the site gave them; a chapter whose slug number changes later has to be refreshed from
  the title's chapter list (the Kotlin does the same).

## Test

```
cargo test -p kuro-mangathemesia
cargo test -p kuro-lavascans
```

The fixtures (in `lib/mangathemesia/fixtures/lavascans`) were saved from the live site on 2026-10-25, without their
scripts and styles.
