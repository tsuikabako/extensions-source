# MangaGoku

MangaGoku (<https://www.mangagoku.com>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops).

MangaGoku is the Mangakakalot catalogue under a new name and a new layout (November 2026): the same titles, addresses,
chapter API and image servers. The `mangakakalot` source reads the mirror that keeps the old layout; this one reads the
new one, so either can stop working without the other.

## Licence and origin

Licensed under the Apache License 2.0. The selectors follow the "modern" layout of the keiyoushi MangaBox base
(Apache-2.0). The source id is `MD5("mangagoku/en/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`8821297132723982231`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests (the MangaBox sources allow 2 a second) |
| `request-headers` | the image headers: the site as Referer (the CDN answers 403 without one, covers included) |
| `popular` | `GET /manga-list/hot-manga?page=N` |
| `latest` | `GET /manga-list/latest-manga?page=N` |
| `search` | `GET /search?q={query}&page=N`; no text: the hot list |
| `details` | `GET /manga/{slug}`: title, author, status, genres, summary, cover |
| `chapters` | `GET /api/manga/{slug}/chapters?limit=-1`: every chapter with its date, newest first |
| `pages` | `GET /manga/{slug}/{chapter}`: the image of each page of the reader, in order |

A listing has a next page when its pager has a "Next page" link.

## Not ported

- The genre, status and sort filters.
- The other CDNs: the Kotlin retries a failing image on other CDNs, the app cannot yet.
- The option that merges strip images into taller ones.

## Test

```
cd sources/mangagoku && cargo test
```

The fixtures were saved from the live site on 2026-11-21 and trimmed to a few cards, chapters and pages.
