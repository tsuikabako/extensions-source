# Torrentio Anime

Torrentio Anime as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime ops): anime titles from
AniList, and for each episode the torrents Torrentio (<https://torrentio.strem.fun>) finds on the public trackers.

**Needs Kurobayomi 0.2.6 or newer**: the videos are magnet links, which only the app's built-in torrent engine plays (an
older app refuses them).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `all/torrentioanime` extension (Apache-2.0, version 22),
torrents only. The source id is the Kotlin one, from the Kotlin's name: `MD5("torrentio anime (torrent / debrid)/all/1")`
taken as its first 8 bytes, big-endian, with the top bit cleared: `7490206640756700883`. Titles are stored by their
AniList id (`154587`) and episodes as Torrentio's stream path (`/stream/series/kitsu:46474:1.json`,
`/stream/movie/kitsu:<id>.json`), as the Kotlin keeps them.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 300 ms between requests |
| `popular` | AniList, trending, finished or airing, 30 a page |
| `latest` | AniList's airing schedule, latest airings first (Chinese and adult titles left out, each title once) |
| `search` | AniList by text, most popular first |
| `details` | AniList by id: romaji title, cover, description, status, tags and genres, studios; release season, type and episode count under the description |
| `episodes` | `api.ani.zip/mappings?anilist_id=`: the Kitsu id (else Kitsu's own AniList mapping) and, for a TV, ONA or OVA, every aired episode ("Episode N: title", air date), newest first; for a movie one "Movie". Specials are left out |
| `videos` | `torrentio.strem.fun/<options>/stream/...json`: one video per stream in Torrentio's order (best quality first), its `videoUrl` a magnet link with `&index=<fileIdx>`, the trackers Torrentio names for it and a fixed list of anime trackers |

Torrentio's options are the Kotlin defaults: its default providers; no 720p, 480p, rips, screeners, cams or unknown
quality; sorted by quality then seeders.

## Not ported

- Debrid (Real-Debrid, AllDebrid, Premiumize, TorBox, ...): it needs the user's account key, so a source settings screen;
  a power-user feature left for later.
- The settings (providers, excluded qualities, languages, sorting, title language, upcoming episodes, dub and codec
  preferences): fixed at the Kotlin defaults.
- The AniList filters (sort, format, genres, year, season, status).
- The public tracker list the Kotlin fetched from GitHub for every video: a fixed list is built in instead.

## Test

```
cargo test -p kuro-torrentioanime
```

The fixtures were saved from the live services on 2026-10-29.
