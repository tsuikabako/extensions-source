# Atsumaru

Atsumaru (<https://atsu.moe>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops), English.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/atsumaru` extension (Apache-2.0). The API paths, the
search's fixed filters, the description, genre, people, status and chapter-order rules follow it. The source id is the
Kotlin one: `MD5("atsumaru/en/2")` taken as its first 8 bytes, big-endian, with the top bit cleared: `2327480808438768017`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests (the Kotlin keeps to 2 a second) |
| `request-headers` | the page-image headers: `Accept: image/avif,image/webp,*/*` |
| `popular` | `GET /api/home2/popular` (today's most read; manga, manhwa, manhua, OEL comics), 40 a page |
| `latest` | `GET /api/home2/recentlyUpdated`, 40 a page |
| `search` | `GET /collections/manga/documents/search` (Typesense): no hidden titles, novels or explicit ratings; no text lists everything |
| `details` | `GET /api/manga/page?id={id}`: rating, year, views, synopsis and other names in the description; type and genres; authors and artists; status |
| `chapters` | `GET /api/manga/page` (for the groups' names), then `GET /api/manga/allChapters?mangaId={id}`: every group's upload, highest number first |
| `pages` | `GET /api/read/chapter?mangaId=..&chapterId=..`; the images are AVIF on `cdn.atsu.moe` |

"Show adult" adds `adult=1` to the lists and drops `isAdult:=false` from the search, as the Kotlin's 18+ setting does.
Stored addresses are the Kotlin's: a title is its id (`2VgNt`), a chapter `<manga id>/<chapter id>`.

## Not ported

- The filters (genres, tags, type, status, year, chapters, sort, official translations), related titles, and the
  settings to exclude genres or scanlators and to keep one upload per chapter.
- The cover is the original poster (jpg or webp), not the large AVIF copy the Kotlin prefers.

## Test

```
cargo test -p kuro-atsumaru
```

The fixtures were saved from the live API on 2026-10-25 (trimmed to the fields read).
