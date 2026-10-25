# Latanime

Latanime (<https://latanime.org>) as an anime source plug-in for Kurobayomi: anime in Spanish, dubbed (Latino,
Castellano) and subbed.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `es/latanime` extension (Apache-2.0). The source id is the
Kotlin one: `MD5("latanime/es/1")` taken as its first 8 bytes, big-endian, with the top bit cleared: `7257892387451890645`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest (the site has none); 1 s between requests |
| `popular` | `GET /emision?p=N`: the airing titles; a next page when the pager has a page after the active one |
| `search` | `GET /buscar?q=...&p=N` (an empty search shows the airing titles) |
| `details` | the title page: title, summary, genres |
| `episodes` | the same page, newest first, numbered from "Capitulo N" |
| `hosters` | the episode page: one server per `a.play-video` whose embed address (base64 in `data-player`) an extractor reads |
| `videos` | the server's embed page, read by [`lib/extractors`](../../lib/extractors): DoodStream (playmogo and other Dood domains), Hexload, MixDrop, Mp4Upload |

The manifest lists the site and every host in `kuro_extractors::HOSTS`; a test checks that it does. The videos load in
the app, from any host, with the headers each extractor gives.

## Not ported

- The search filters (year, genre, letter).
- The servers of video hosts that move to new domains all the time or need more than a page read: VOE, Filemoon,
  StreamWish, VidGuard, Okru, Uqload, YourUpload, MEGA, savefiles. An episode shows only the servers that play.

## Test

```
cargo test -p kuro-latanime -p kuro-extractors
```

The fixtures were saved from the live site and the video hosts on 2026-10-25. Checked live the same day through the
app's runtime: every op, and a video from each of the four servers.
