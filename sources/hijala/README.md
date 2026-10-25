# Hijala

Hijala (<https://hijala.com>), an Arabic manhwa and manhua translation site, as a manga source plug-in for Kurobayomi
(ABI 1.5, `wit/kuro-source.wit`, manga ops). A MangaThemesia site: the ops, URLs and parsers are the shared
[`lib/mangathemesia`](../../lib/mangathemesia) crate, and `src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/hijala` extension and its `mangathemesia` theme
(`lib-multisrc/mangathemesia`, Apache-2.0). The source id is the Kotlin one: the extension sets `versionId = 2` (the site
moved from ZeistManga back to MangaThemesia), so it is `MD5("hijala/ar/2")` taken as its first 8 bytes, big-endian, with
the top bit cleared: `917436262447415426`. Titles and chapters are stored by their path on the site (`/lookism/`,
`/lookism-630/`), as the Kotlin stores them.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `request-headers` | the site as Referer (the images load without it too) |
| `popular` | `GET /manga/?page=N&order=popular`; a next page when the page has the "next" link |
| `latest` | `GET /manga/?page=N&order=update` |
| `search` | `GET /manga/?title=...&page=N` |
| `details` | the title page: title, author, artist, summary and alternative names, genres and type, status, cover |
| `chapters` | the same page, newest first, with dates in Arabic (`أكتوبر 22, 2026`) |
| `pages` | the chapter page: the reader's images, which this site keeps in a `<noscript>` |

## Not ported

- The search filters (genre, status, type, order, author, year).
- The view counter and related titles.
- **Split images.** The Kotlin joins chapters whose images come in left and right halves (`#chapter-pages-js-before`)
  into one picture. A plug-in cannot fetch a second image to join, so such a chapter answers a clear error. None of the
  chapters checked on 2026-10-25 were split.

## Test

```
cargo test -p kuro-mangathemesia
cargo test -p kuro-hijala
```

The fixtures (in `lib/mangathemesia/fixtures/hijala`) were saved from the live site on 2026-10-25, without their
scripts and styles.
