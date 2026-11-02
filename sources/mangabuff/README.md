# MangaBuff

MangaBuff (<https://mangabuff.ru>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops),
Russian.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/mangabuff` extension (Apache-2.0). The pages read,
the selectors and the title, description, genre and status rules follow it. The source id is the one the Kotlin gets,
`4008409601887986192`: `MD5("mangabuff/ru/1")` taken as its first 8 bytes, big-endian, with the top bit cleared.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests (the Kotlin allows one a second to title pages and the chapter list) |
| `popular` | `GET /manga?sort=real_views&page=N` |
| `latest` | `GET /manga?sort=updated_at&page=N` |
| `search` | `GET /search?type=manga&q=...&page=N`; a text under 3 characters is refused, as the Kotlin. No text lists Popular |
| `details` | the title page: the description with the rating, views, favourites and other names; type, year and genres as genres; status |
| `chapters` | the title page, newest first. It shows only 100 chapters; when it says there are more (`.load-chapters-trigger`), `POST /chapters/load` with the manga id and the page's CSRF token gives the rest (a 419 is retried once with a fresh token) |
| `pages` | the reader page, `.reader__pages img` (`data-src`, else `src`) |

Stored addresses are the site's paths: a title is `/manga/<slug>`, a chapter `/manga/<slug>/<volume>/<chapter>`, as the
Kotlin keeps them. A title the site shows only to signed-in readers answers 404; the app cannot sign in to MangaBuff, so
those titles say so.

## Not ported

- The filters (genres, tags, type, status, age, rating, year, chapter count, sort): the app shows no source filters yet.
- The address deep link and the setting that turns animated GIF pages into still images (the app shows GIFs).

## Test

```
cargo test -p kuro-mangabuff
```

The fixtures were saved from the live site on 2026-11-02.
