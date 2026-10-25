# AniWorld

AniWorld (<https://aniworld.to>) as an anime source plug-in for Kurobayomi: anime in German, dubbed and subbed (and some
with English subtitles).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `de/aniworld` extension (Apache-2.0). The source id is the
one the Kotlin extension sets in its code: `8286900189409315836`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; 0.5 s between requests |
| `popular` | `GET /beliebte-animes`: the cards of `div.seriesListContainer` (one page) |
| `latest` | `GET /neu`, the same cards |
| `search` | `GET /ajax/seriesSearch?keyword=...` (JSON, one page), as the site's search box asks it |
| `details` | the title page: title, cover, summary, genres, producers |
| `episodes` | every season page of the title (and its films page), newest first: "Staffel 2 Folge 3 : ...", "Film 1 : ..." |
| `hosters` | the episode page: one server per Doodstream or Vidmoly link, named with its language (Deutscher Dub, Deutscher Sub, Englischer Sub); the server's address is the site's `/redirect/<id>` |
| `videos` | the redirect leads to the host's player page, read by [`lib/extractors`](../../lib/extractors); each video's title starts with the language |

The manifest lists the site and every host in `kuro_extractors::HOSTS` (the redirects land on playmogo.com and
vidmoly.biz, both listed); a test checks that it does.

## Not ported

- The servers VOE and Filemoon (they move to new domains all the time), Vidoza and Streamtape (the site offers them no
  more).
- The preferences (preferred language and server, excluded servers): each server is named with its language instead.
- The DDoS-Guard cookie step: the site does not ask for it today.

## Test

```
cargo test -p kuro-aniworld
```

The fixtures were saved from the live site on 2026-10-25 (scripts removed). Checked live the same day through the app's
runtime: every op, and a video from each of the six servers of an episode (Doodstream and Vidmoly in three languages).
