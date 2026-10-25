# WcoAnimeSub

WcoAnimeSub (<https://www.wcoanimesub.tv>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`,
anime ops): English subbed anime from the WCO family, mp4 up to 1080p. A WCO site: the ops, the player and the parsers
are the shared [`lib/wco`](../../lib/wco) crate, and `src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `en/wcoanimesub` extension and its `wcotheme` theme
(`lib-multisrc/wcotheme`, Apache-2.0). The source id is the Kotlin one: the extension sets none, so it is
`MD5("wcoanimesub/en/1")` taken as its first 8 bytes, big-endian, with the top bit cleared: `665629163594938512`. Stored
addresses are the site's paths: `/anime/<slug>` for a title, `/<slug>-episode-<n>-english-dubbed` for an episode.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests |
| `popular` | `GET /`: the home page's "Recently Added Series", one page |
| `latest` | `GET /`: the home page's "Recent Releases" (episode pages, kept as titles as the Kotlin does), one page |
| `search` | `POST /search` with `catara=...&konuara=series`, one page (an empty search is `popular`). The Kotlin turns text search off ("broken server side"); it answered on 2026-10-25, so it is on |
| `details` | the title page: title, summary, genres, cover (an episode page: its series' name) |
| `episodes` | the same page, newest first (WCOStream's episode pages list oldest first and are turned), numbered from "Season S Episode N" ((S - 1) * 100 + N); an episode page is its own one episode |
| `hosters` | the episode page: one server per WCO player (`embed.wcostream.com`), from its iframe or from the script that writes it |
| `videos` | the player: its page, the "clear" ad-check beacon, a 13 s wait (the server refuses the player before its 12 s countdown), the player page (the newer one, asked up to twice more 2-3 s apart while it answers "busy" (503); then the older one, which the Kotlin asks first), then its JSON: up to three mp4 files, 1080p, 720p, 480p |

The site serves only one kind of title (dubbed or subbed); a title of the other kind answers with a 301 to
`www.wco.tv` or `www.wcoanimedub.tv`, so the manifest lists those hosts too.

The wait is spent on the guest's clock (ABI 1.5 has no sleep import): about 13 s of the call's 30 s of run time. The
file server plays a video only to the User-Agent that asked the player for it, so the player's requests and each video
carry one fixed desktop User-Agent (the Kotlin's).

## Not ported

- The genre filter.
- The settings: preferred quality (the player starts with the best) and the countdown length.
- The premium player (`vhs.watchanimesub`): no episode used it on 2026-10-25.

## Test

```
cargo test -p kuro-wcoanimesub -p kuro-wco
```

The theme's fixtures were saved from WcoTv and WCOStream on 2026-10-25.
