# ReadManga

ReadManga (<https://a.zazaza.me>), a Russian manga site of the GroupLe network (Usagi, ReadManga, SelfManga), as a manga
source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). The ops, URLs and parsers are the shared
[`lib/grouple`](../../lib/grouple) crate; `src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/readmanga` extension (Apache-2.0, version 48),
which is the keiyoushi GroupLe multisrc (`lib-multisrc/grouple`, base version 43) with site id 1 and no overrides. The
base address is the extension's (`https://a.zazaza.me`). The source id is the Kotlin one, which the extension sets by
hand (`id = 5L`, the id of the old built-in ReadManga source) instead of deriving it from the name: `5`. Manga urls are
the site's paths, chapter urls carry the Kotlin's `?mtr=true`, so Aniyomi backups map onto it.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests (the Kotlin's `rateLimit(2)`) |
| `request-headers` | the site as Referer and the User-Agent `arora` |
| `popular` | `GET /api/catalog/search?offset=N&sortType=RATING`, 50 titles a page |
| `latest` | the same with `sortType=DATE_UPDATE` |
| `search` | the same with `q=...`, sorted by rating |
| `details` | `GET /{title}`: title, authors, artists, category, age rating and tags as genres, status, rating, summary and alternative names in the description, cover |
| `chapters` | the same page: the chapter table, newest first, with number, date, translator and the Kotlin's name clean-up |
| `pages` | `GET /{title}/volN/M?mtr=true`: the image addresses from `rm_h.readerInit`, the token query dropped for `one-way.work` |

Every request sends the User-Agent `arora`, the GroupLe default (the site also answers a browser one).

## Not ported

- **Sign-in.** The Kotlin can sign in automatically through `3.grouple.co`; this port cannot. Titles the site hides from
  guests (`blockedForAnonymous`; on 2026-10-25 some popular titles, such as `/podniatie_urovnia_v_odinochku__A5ea4`, but
  none of the six latest) answer the chapter list with the Kotlin's "sign in" error, and a chapter page that answers 404
  gives it too. Most titles are readable as a guest.
- **The user hash** (`?d=...`) the Kotlin adds for signed-in users.
- **The search filters** (sort, genres, tags, categories, statuses, years): the Kotlin fetches them from the site at run
  time.
- **The User-Agent setting** and **related titles**.
- **Paid chapters** answer the Kotlin's "this chapter is paid" error.

## Test

```
cd lib/grouple && cargo test
cd sources/readmanga && cargo test
```

The parsers are tested in `lib/grouple` with Usagi's saved pages; the site was checked live on 2026-10-25 (see above).
