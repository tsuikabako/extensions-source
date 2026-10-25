# MangaTime

MangaTime (<https://mangatime.org>), an Arabic site hosting many translation teams, as a manga source plug-in for
Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). It reads the site's tRPC API.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/mangatime` extension (Apache-2.0). The source id is
the Kotlin one: `MD5("mangatime/ar/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`215553151312092548`. Titles are stored as the Kotlin stores them, `/<type>/<slug>#<series id>` (the slug for the
details, the id for the chapters), and chapters as `/<type>/<slug>/chapter/<number>`.

## Ops

Every request is a batched tRPC query, `GET /api/trpc/<procedure>?batch=1&input={"0":{"json":...}}`.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `popular` | `search.searchSeries` sorted by `popularity`, 24 a page, the API's `hasMore` |
| `latest` | the same sorted by `recent` |
| `search` | the same sorted by `popularity` with `query` |
| `details` | `content.getSeriesBySlug`: title, author, artist, summary, genres and type, status, cover |
| `chapters` | `content.getChapters` with the series id: every chapter, newest first, with number and date |
| `pages` | `content.getChapterPages` with the slug and the chapter number (`364.5` too); a locked chapter is an error |

The page images need no Referer.

## Not ported

- The view counter (`content.trackView`) the Kotlin posts when a chapter opens.
- Locked chapters: they stay in the list (as in the Kotlin) and answer "this chapter is locked" when opened. None were
  locked in the checks on 2026-10-25.

A chapter without a title of its own is named "Chapter N" (the Kotlin's form; the Kotlin fails on such chapters).

## Test

```
cargo test -p kuro-mangatime
```

The fixtures were saved from the live API on 2026-10-25 and trimmed to a few titles and chapters.
