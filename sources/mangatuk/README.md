# MangaTuk

MangaTuk (<https://mangatuk.com>), an Arabic manhwa, manga and manhua site, as a manga source plug-in for Kurobayomi
(ABI 1.5, `wit/kuro-source.wit`, manga ops). It reads the site's API, `api.mangatuk.com`.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/mangatuk` extension (Apache-2.0). The source id is
the Kotlin one: the extension sets `versionId = 2`, so it is `MD5("mangatuk/ar/2")` taken as its first 8 bytes,
big-endian, with the top bit cleared: `6924751245735287034`.

Titles are stored as their path on the site, `/series/<slug>`, and chapters as `/series/<slug>/<chapter slug>#<chapter
id>` (the id for the pages). The Kotlin stores the bare series and chapter ids; those are read too, so restored backups
work.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `popular` | `GET /api/catalog/search?sort=popular&limit=30&offset=N`; a next page while the `total` is not reached |
| `latest` | the same with `sort=latest_updates` |
| `search` | the same with `q=...`, sorted by popularity |
| `details` | `GET /api/catalog/series/by-slug/<slug>`: title, author, summary and other names, type and genres, status, cover |
| `chapters` | the same answer: the free chapters, newest first, with number and date |
| `pages` | `GET /api/catalog/chapters/<id>/pages`: the image addresses |

The page images (`content.mangatuk.com`) need no Referer.

## Not ported

- **Coin-locked chapters** are left out of the list, as in the Kotlin.
- The search filters (sort, status).
- The site marks some chapters adult (`isAdult`); the Kotlin shows them all, and so does this port.

## Test

```
cargo test -p kuro-mangatuk
```

The fixtures were saved from the live API on 2026-10-25 and trimmed to a few titles and chapters.
