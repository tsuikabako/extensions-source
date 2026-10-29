# Nyaa (Torrent)

Nyaa (<https://nyaa.si>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime ops). Every
title is one torrent; its episodes are the video files inside it.

**Needs Kurobayomi 0.2.6 or newer**: the videos are magnet links, which only the app's built-in torrent engine plays (an
older app refuses them).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `all/nyaatorrent` extension (Apache-2.0, version 4), its
first source only: nyaa.si. The second, Sukebei, is an adult site and is left out. The source id is the Kotlin one:
`MD5("nyaa (torrent)/all/1")` taken as its first 8 bytes, big-endian, with the top bit cleared: `5310001068353260432`.
Titles are stored as the site's torrent path (`/view/2115493`), as the Kotlin keeps them.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests |
| `popular` | `/?f=0&c=1_0&s=seeders&o=desc&p=N`: the anime category by seeders |
| `latest` | `/?f=0&c=1_0&p=N` |
| `search` | `/?f=0&c=1_0&q=...&p=N`; `id:<n>` or a `https://nyaa.si/view/<n>` link opens that torrent |
| `details` | the torrent page: category, seeders, leechers and size as genres, the submitter, the description, the first image in it as the cover |
| `episodes` | the torrent page's magnet link (info hash, trackers) and `/download/<n>.torrent` (the file list, `lib/torrent`): one episode per video file, the file name as its name, its size as the scanlator, newest first. When the `.torrent` cannot be had and the page lists one file, that file plays at index 0 |
| `videos` | the episode's magnet link, `magnet:?xt=urn:btih:<hash>&dn=<name>&tr=...&index=<N>` (`index`: the file inside the torrent), titled with the height its file name states ("1080p") or "Auto" |

## Not ported

- The sort, filter (no remakes, trusted only) and sub-category filters.
- The settings (custom domain, full path as the episode name).
- Episode urls are magnet links as in the Kotlin, but with the torrent's name as `dn` and the trackers percent-encoded,
  so an Aniyomi backup's episode history may not match these episodes.

## Test

```
cargo test -p kuro-nyaa
```

The fixtures were saved from the live site on 2026-10-29; the `.torrent` has its piece hashes blanked to keep it small.
