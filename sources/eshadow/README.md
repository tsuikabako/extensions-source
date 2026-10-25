# EShadow

EShadow (<https://eshadow.net>), a small Arabic manga translation site, as a manga source plug-in for Kurobayomi
(ABI 1.5, `wit/kuro-source.wit`, manga ops). It reads the site's JSON API.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/eshadow` extension (Apache-2.0). The source id is
the Kotlin one: `MD5("eshadow/ar/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`8617573161604126293`.

Titles are stored as `/manga/<slug>#<id>` (their page on the site; the API knows them only by id), chapters as their
page, `/read/<chapter id>`. The Kotlin stores the bare ids; those are read too, so restored backups work.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest (the site has one list, as in the Kotlin); 1 s between requests |
| `popular` | `GET /api/manga?page=N`; a next page while `page × limit` is below `total` |
| `search` | `GET /api/manga?page=N&query=...` |
| `details` | `GET /api/manga/<id>`: title, author, summary, status, cover |
| `chapters` | the same answer: the chapters, highest number first, with dates |
| `pages` | `GET /api/chapters/<id>`: the chapter's image addresses |

The Kotlin keeps a chapter's images with the chapter when it lists them; a plug-in has no such store, so `pages` asks the
API for the one chapter. The page images (an `r2.dev` bucket) need no Referer.

On 2026-10-25 the site listed 7 titles, some of them without chapters.

## Test

```
cargo test -p kuro-eshadow
```

The fixtures were saved from the live API on 2026-10-25.
