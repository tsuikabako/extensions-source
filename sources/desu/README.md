# Desu

Desu (<https://desu.uno>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops), Russian.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/desu` extension (Apache-2.0). The API paths, the
title, description, genre and status rules follow it. The source id is the one the Kotlin sets,
`6684416167758830305`, which is also `MD5("desu/ru/1")` taken as its first 8 bytes, big-endian, with the top bit cleared.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 2 s between requests (the Kotlin allows one every 2 seconds) |
| `request-headers` | the page-image headers: `Referer: https://desu.uno/` (some image hosts refuse a browser without it) |
| `popular`, `latest` | `GET /api/manga/catalog/?limit=20&page=N`: the catalog's one order, latest updates first (the Kotlin shows it for both). Titles rated 18+ are left out unless "Show adult" is on |
| `search` | the site's quick search, `GET /manga/search/?q=...` (HTML, at most 20 titles, one page): the catalog API ignores search text. No text lists the catalog |
| `details` | `GET /api/manga/{id}/`: English title, the Russian one, the rating and other names in the description; kind, age rating and genres as genres; status |
| `chapters` | `GET /api/manga/{id}/chapters`, newest first, named `<volume>. Глава <number> <title>` |
| `pages` | `GET /api/manga/{id}/chapters/{chapter}` |

Stored addresses: a title is `/<id>` (as the Kotlin). A chapter is `/<manga id>/chapters/<chapter id>`: the Kotlin keeps only
the chapter id and the manga in a side memo the app does not have, and the API needs both.

## Not ported

- The filters (the Kotlin has them commented out: the API ignores them) and the title-language setting (English first).
- Adult titles in search: the quick search does not say a title's age rating, so they are not left out there.

## Test

```
cargo test -p kuro-desu
```

The fixtures were saved from the live site on 2026-10-25.
