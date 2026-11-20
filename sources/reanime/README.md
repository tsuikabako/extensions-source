# Re:ANIME

Re:ANIME (<https://reanime.cz>) as an anime source plug-in for Kurobayomi: anime subbed and dubbed, each episode played
from its original MKV file (1080p for most), with every audio track and subtitle inside the file.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `en/reanime` extension (Apache-2.0). The source id is the
Kotlin one: the extension sets none, so it is `MD5("re:anime/en/1")` taken as its first 8 bytes, big-endian, with the
top bit cleared: `7105156973687362431`. Stored addresses are the Kotlin ones: the title's id on the site
(`mushi-shi-the-next-passage-2-qazz9g`) for a title, `<title id>/ep-<n>` for an episode.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest offered; 200 ms between requests (the Kotlin allows 5 a second) |
| `popular` | `GET /api/v1/search?sort=popularity_desc&limit=36&offset=..` |
| `latest` | `GET /api/v1/home/latest-aired?limit=12&lang=`, a further page with the `cursor` the page before gave |
| `search` | `GET /api/v1/search?q=...&limit=36&offset=..` (an empty search is `popular`) |
| `details` | `GET /api/v1/anime/<id>`: title (English, else romaji, else native), cover, status, genres and tags, main studio; the description is the Kotlin's (score as stars, synopsis, one fact a line, links to the trackers, the legal streams and the trailer) |
| `episodes` | `GET /api/v1/anime/<id>/episodes?limit=2000`; newest first, with air date, filler mark and synopsis |
| `hosters` | the title's ids (kept from its details), then `GET /api/flix/<AniList id>/<episode>?mal=..`: one hoster per file, "FlixCloud (Sub & Dub)" |
| `videos` | `GET https://flixcloud.cc/d/<file>/__data.json`: one video, the file's download address, "1080p MKV" |

Titles the site rates "Rx - Hentai" (or gives that genre) are left out of the lists while the app's adult switch is off.

The latest list is paged by a cursor. The cursor of the page last read is saved in a preference, so the next page is one
call; without it (the app restarted) the list is read again from page 1. The ids of the last title asked about are
saved too, so the next episode of that title asks its servers at once.

The site lists a file once for each of its players and audio types ("HD-1" sub, "HD-1" dub, "HD-2", "H-SUB"); they all
lead to the one file, so they are one hoster, named by the audio types.

The manifest lists `reanime.cz` and `flixcloud.cc`. The file is on `fetch*.flixcloud.cc`, which redirects to a signed
link on a download host; the app loads it.

## The video: the file, not the site's HLS stream

The site's player streams the file in HLS from FlixCloud. Every playlist of that stream (the master and each quality)
is scrambled with a 32-byte key, and the key is computed by a small WebAssembly program that the host writes anew for
each player page, with the playlist address itself behind three more layers. A plug-in can only hand the app an
address, and the app's player cannot read a scrambled playlist, so that stream cannot be offered. The Kotlin plays it
through an interceptor inside the app; it also offers the host's direct download, which is what this source uses:

- `https://flixcloud.cc/d/<file>/__data.json` names the file's id, its resolution, the download host and a token
  (valid 6 hours, tied to the asking IP address).
- `<download host>/download/<file id>?token=<token>` redirects to the MKV file under a signed link.

Checked on 2026-11-20 with ffprobe and ffmpeg 9.0.2 on one episode: Matroska, H.264 1080p, Japanese and English AAC
audio, 12 subtitle tracks with their fonts; range requests are answered, and 20 seconds decoded from the 20-minute mark.

**Known limit: the signed link lives 15 minutes.** After that it answers 404 (checked). ffmpeg keeps the address a
redirect led to, so a jump to a part of the episode that is not buffered yet, made more than 15 minutes after the
episode started, fails; opening the episode again gets a new link. Playing through, and jumps inside what is buffered,
do not ask the link again. Not checked in the app. The fix is in the app: ask the first address again when the signed
one has expired.

Not checked: whether a file nobody asked for before must first be prepared by the host. The Kotlin waits on
`/download/<id>/progress` for that; it is an event stream that stays open, which the app's `http` import would wait on,
so this source does not ask it. Both files tried (a new episode and one from 2014) were ready at once.

## Not ported

- The HLS stream (above), with its subtitle list, fonts and chapter marks.
- The search filters.
- The settings: title language, preferred quality, server and audio, hidden servers and audio types.
- The related titles and recommendations.
- The episode preview pictures (`/api/thumbnails`): the app's episode has no field for them.
- The mirrors (`reanime.bz`, `reanime.wtf`).

## Test

```
cargo test -p kuro-reanime
cargo test -p kuro-reanime -- --ignored    # the same ops against the live site; needs curl
```

The fixtures were saved from the live site on 2026-11-20; the token in `download.json` is a made-up one (the real one
names the asking IP address). Checked live the same day with the ignored test, which runs the ops through
`kuro_guest::testing::LiveHost` (real requests, the manifest's `hosts` rule kept): popular, latest pages 1 and 2,
search, details, the 10 episodes of a title, its file and the file's download address. Not checked: the built plug-in
in the app (this machine has no WASM target; CI builds it).
