# MonosChinos

MonosChinos (<https://monoschinos.st>) as an anime source plug-in for Kurobayomi: anime subbed in Spanish.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `es/monoschinos` extension (Apache-2.0), adapted to the
site's new layout. The source id is the one the Kotlin extension sets in its code: `6957694006954649296`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; 0.5 s between requests |
| `popular` | `GET /animes?p=N`: the cards `article > a.card-wrap`; a next page when the pager has a `rel=next` link |
| `latest` | `GET /`: the newest episodes, each as its title (`/anime/<slug>-sub-espanol`, as the Kotlin builds it), once; one page |
| `search` | `GET /buscar?q=...&p=N` (an empty search shows the popular list) |
| `details` | the title page: title, cover, summary, genres, status |
| `episodes` | the title page's `section.caplist[data-ajax]`, then that address (`GET`, JSON: every episode number at once); `/ver/<slug>-episodio-<n>`, newest first |
| `hosters` | the episode page: one server per `button.play-video` whose embed address (base64 in `data-player`) an extractor reads |
| `videos` | the server's embed page, read by [`lib/extractors`](../../lib/extractors): DoodStream, MixDrop, Mp4Upload (and Okru, Uqload when an episode has them) |

DoodStream embeds come on domains the extractors do not list (dsvplay.com, which no longer resolves); every Dood domain
serves the same ids, so the source asks playmogo.com. The manifest lists the site and every host in
`kuro_extractors::HOSTS`; a test checks that it does.

## Not ported

- The search filters (genre, year, type, status, order).
- The preferences (preferred server and quality).
- The servers of video hosts that move to new domains all the time or are not read: VOE, Filemoon, StreamWish,
  LuluStream, MEGA, savefiles, Streamtape.

## Test

```
cargo test -p kuro-monoschinos
```

The fixtures were saved from the live site on 2026-10-25. Checked live the same day through the app's
runtime: every op, and a video from each of the three servers of an episode (DoodStream via playmogo.com, MixDrop,
Mp4Upload).
