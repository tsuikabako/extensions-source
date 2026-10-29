# SubsPlease

SubsPlease (<https://subsplease.org>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime
ops): the group's English-subbed releases, through the site's JSON API.

**Needs Kurobayomi 0.2.6 or newer**: the videos are magnet links, which only the app's built-in torrent engine plays (an
older app refuses them).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `all/subsplease` extension (Apache-2.0, version 7). The
source id is the Kotlin one: `MD5("subsplease/all/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`8766955949256845198`. Shows are stored as `/shows/<slug>` and episodes as the show API with the episode's number
(`/api/?f=show&tz=Europe/Berlin&sid=1121&num=10`), as the Kotlin keeps them.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 300 ms between requests |
| `popular` | `/api/?f=schedule`: the week's airing shows, Monday first (one page) |
| `latest` | `/api/?f=latest` (page 1) and `&p=1` (page 2; the API has no third page): newest release first, one per show |
| `search` | `/api/?f=search&s=...` (one page); no text shows the schedule |
| `details` | the show page: title, synopsis, poster |
| `episodes` | the show page's `sid`, then `/api/?f=show&sid=...`: "Episode N", the release date, newest first. Batches are left out, as in the Kotlin |
| `videos` | the same answer, the episode's downloads: one video per resolution (1080p first and preferred), its `videoUrl` the API's magnet link. Each release is a one-file torrent, so the link needs no `index` |

## Not ported

- The settings: preferred quality (1080p first), and the debrid provider and token (Torrentio's debrid resolver; a
  power-user feature left for later).

## Test

```
cargo test -p kuro-subsplease
```

The fixtures were saved from the live site on 2026-10-29.
