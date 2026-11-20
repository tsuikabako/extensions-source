# Anikoto

Anikoto (<https://anikototv.bz>) as an anime source plug-in for Kurobayomi: anime subbed and dubbed in English, each
server of an episode one HLS stream (up to 1080p) with subtitles in several languages.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `en/anikoto` extension and its `anikototheme` library
(Apache-2.0). The source id is the Kotlin one: the extension sets none, so it is `MD5("anikoto/en/1")` taken as its
first 8 bytes, big-endian, with the top bit cleared: `4697393375201558791`. Stored addresses are the Kotlin ones:
`/watch/<slug>` for a title (the Kotlin adds `#<site id>` after the details are read; both forms are accepted), and
`<server ids>&epurl=/watch/<slug>/ep-<n>&mal=..&slug=..&ts=..` for an episode.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest offered; 200 ms between requests (the Kotlin allows 5 a second) |
| `popular` | `GET /most-viewed/?page=N` |
| `latest` | `GET /latest-updated/?page=N` |
| `search` | `GET /filter?keyword=...&page=N` (an empty search is `popular`) |
| `details` | the title page: title (English, else romaji), cover, status, genres, studio; the description is the score as stars, the synopsis, the other facts of the page, studio and producers, the other names |
| `episodes` | the title page for the site's id (not asked when the address carries it), then `GET /ajax/episode/list/<id>`; newest first, with the site's upload time |
| `hosters` | `GET /ajax/server/list?servers=<ids>`: one server per name and audio type, "Vidstream-2 (Sub)" |
| `videos` | `GET /ajax/server?get=<link id>` for the player page, the page for its file id, `GET <player>/stream/getSources?id=..&st=other`: one video, the HLS master playlist (its qualities are inside it), with every subtitle track |

The players are MegaPlay pages (`megaplay.buzz`, and its mirror `vidtube.site` for the "VidPlay" server). `getSources`
answers with the subtitle tracks and `enc`: the playlist address in AES-256-CBC, under a key and IV that are constants
of the player's own script (`lib/e1-player.min.js`, read 2026-11-20). An answer with the address in clear is read too.

The manifest lists `anikototv.bz`, `megaplay.buzz` and `vidtube.site`. The playlists, segments and subtitles are on CDN
hosts that change (`fetch.nexabloom.top`, `*.crystaldrift.world`, `*.questforwifi.site` on 2026-11-20); the app loads
them. Each video carries `Referer` and `Origin` of its player: the CDN answers 403 without the Referer.

## Not offered: the server on the image CDN ("HD-1")

A player address with `s=tcdn` (the site's "HD-1") serves the same episode from an image host: each segment is a
one-pixel PNG image followed by the MPEG-TS data. ffmpeg, which the app's player is built on, reads such a segment as a
picture (ffprobe 9.0.2 on 2026-11-20: the stream's codec is "png"), so that video cannot play. `videos` answers such a
server with an empty list, without asking its player page, and the app leaves the server out. The same episode plays
from the other servers ("HD-2", `s=bcdn`, is a plain stream and is offered).

It can come back when the app's HLS relay drops what stands before the first MPEG-TS sync byte of a segment. Then
delete `megaplay::on_image_cdn` and its use, and bump `version`.

## Not ported

- The search filters.
- The settings: title language (English, then romaji), preferred quality, server and audio type, hidden servers and
  types, where the score goes. The player picks the quality from the playlist.
- The related titles ("watch order", "recommended").
- The extra servers of the third-party "mapper" API (`mapper.nekostream.site`).
- The mirrors (`anikoto.cz`, `anikoto.me`, `anikoto.net`, `anikototv.se`).
- The `vrf` parameter on the episode list and the search: the site answers without it (checked 2026-11-20).
- The CDN token the Kotlin adds to the playlist address: the CDN ignores it (a made-up token, an expired one and none
  at all were all answered 200 on 2026-11-20).
- An episode's upload time read from its tooltip, for an episode without `data-timestamp` (every episode has one).

## Test

```
cargo test -p kuro-anikoto
cargo test -p kuro-anikoto -- --ignored    # the same ops against the live site; needs curl
```

The fixtures were saved from the live site on 2026-11-20. Checked live the same day with the ignored test, which runs
the ops through `kuro_guest::testing::LiveHost` (real requests, the manifest's `hosts` rule kept): popular, latest
page 2, search, details, the 28 episodes of a title, its 8 servers and the playlist of each (6 offered, 2 left out).
With curl and ffprobe 9.0.2: the master playlist, a quality playlist and a segment of each CDN downloaded with the
video's headers; the plain streams are H.264 1080p with AAC. Not checked: the built plug-in in the app (this machine
has no WASM target; CI builds it).
