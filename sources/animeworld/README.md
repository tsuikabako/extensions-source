# ANIMEWORLD.tv

AnimeWorld (<https://www.animeworld.ac>) as an anime source plug-in for Kurobayomi: anime in Italian, subbed (SUB ITA)
and dubbed (ITA).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `it/animeworld` extension (Apache-2.0). The extension
sets no id, so the source id is the derived one: `MD5("animeworld.tv/it/1")` (the name in lower case, the language and
version 1) taken as its first 8 bytes, big-endian, with the top bit cleared: `6601315231151625432`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; 0.5 s between requests |
| `popular` | `GET /filter?sort=6&page=N` (most viewed): the cards of `div.film-list`; a next page when the pager has `#go-next-page` |
| `latest` | `GET /updated?page=N`, the same cards |
| `search` | `GET /filter?keyword=...&page=N` |
| `details` | the title page: title, cover, summary, genres (with the audio language), studio, status |
| `episodes` | the same page: the active server's episode list, newest first |
| `hosters` | the episode page: one server per tab that can play (the site's own "AnimeWorld Server", and DoodStream should the site show it); the server's address is the site's API for that episode, `/api/episode/info?id=...` |
| `videos` | the API answer's file (`grabber`): the site's own server gives an mp4 that plays without headers; a DoodStream file goes through [`lib/extractors`](../../lib/extractors) |

A page answered with 202 and a cookie script (the site's occasional check) is asked again with that cookie, as the
Kotlin does.

The manifest lists the site and every host in `kuro_extractors::HOSTS`; a test checks that it does.

## Not ported

- The search filters (genre, season, year, type, status, studio, sub/dub, audio, order).
- The preferences (preferred quality and server): the site offers one server today.
- The servers Streamtape, StreamHide (VidHide) and VidGuard: no extractor reads them (VidHide moves to new domains,
  VidGuard needs a JavaScript engine), and the site shows none of them today.

## Test

```
cargo test -p kuro-animeworld
```

The fixtures were saved from the live site on 2026-10-25 (the list pages cut to their result grid, scripts removed).
Checked live the same day through the app's runtime: every op, and a video from the site's server.
