# AniZone

AniZone (<https://anizone.to>) as an anime source plug-in for Kurobayomi: anime with many dub languages and soft
subtitles, each episode one HLS stream.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `all/anizone` extension (Apache-2.0). The source id is the
Kotlin one: the extension sets none, so it is `MD5("anizone/all/1")` taken as its first 8 bytes, big-endian, with the
top bit cleared: `954848840440936266`. The language is `all`, as the Kotlin declares. Stored addresses are the Kotlin
ones: `/anime/<slug>` for a title, `/anime/<slug>/<episode>` for an episode.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest offered; 350 ms between requests (the Kotlin allows 3 a second) |
| `popular` | `GET /anime?sort=title-asc` (the whole index, A-Z, the Kotlin's default sort); page N > 1 is one Livewire call (below) |
| `latest` | `GET /`: the home page's "Latest Anime", one page |
| `search` | `GET /anime?search=...&sort=title-asc`, further pages as for `popular` (an empty search is `popular`) |
| `details` | the title page: titles (English, else romaji), cover, status, tags, synopsis |
| `episodes` | the title page's episodes, then `loadPage` calls until the site has no more; specials first, then newest first (the Kotlin's order) |
| `hosters` | the episode page: one server per `setVideo(n)` button, named by the button (a repeated name gets "(2)") |
| `videos` | the episode page's player when it already plays that server, else a `setVideo(n)` Livewire call: one video, the HLS master playlist (qualities and dub languages inside it), with every subtitle track, the site's default first |

The lists and players are Livewire components. Their data is in the page's `x-data` as `JSON.parse('...')`, read by
undoing the JavaScript string escapes (exact, unlike the Kotlin's text replacements). A further page is a POST to
`/livewire/update` with the page's `wire:snapshot`, its CSRF token and `loadPage(<cursor>)`; the session cookie is the
app's. The plug-in saves the state of the last list page in a preference, so the next page is one call; without it
(the app restarted) it reads the list again from page 1. A 419 (expired token) reads the page again and retries once.

The manifest lists only `anizone.to`: the videos and subtitles come from CDN hosts (`*.vid-cdn.xyz`, `*.xin-cdn.xyz`)
that the app loads itself. Each video carries `Referer: https://anizone.to/`; the CDN does not need it today.

## Not ported

- The sort filter.
- The settings: title language (English, then romaji), preferred quality, audio and subtitle language, subtitle count,
  "load all tracks". The player picks quality and audio from the playlist, and every subtitle is offered.

## Test

```
cargo test -p kuro-anizone
```

The fixtures were saved from the live site on 2026-10-25. Checked live the same day through the app's runtime
(`kuro-run`): popular, latest, search, details, the 28 episodes of a title (24 in the page, 4 by a Livewire call), the
servers and the video of an episode; the master playlist, a variant playlist, an AES-128 segment and a subtitle file
downloaded with the video's headers. Checked with curl only: a list's page 2 and `setVideo` for a second server (the
runtime check takes the newest episode of a title, and none had two servers that day); both use the same Livewire call
as the episode pages.
