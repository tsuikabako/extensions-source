# AnimeToast

AnimeToast (<https://www.animetoast.cc>) as an anime source plug-in for Kurobayomi: anime in German, dubbed and subbed.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `de/animetoast` extension (Apache-2.0). The source id is
the Kotlin one: `MD5("animetoast/de/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`2386616842414617638`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest (the Kotlin has none); 0.5 s between requests |
| `popular` | `GET /`: the home page's titles (one page) |
| `search` | `GET /page/N/?s=...`; a next page when the pager has `.nextpostslink` |
| `details` | the title page: title, cover, summary, genres, status (category "Airing") |
| `episodes` | the title page has a tab per server, each listing the episodes (`?link=N`); the list is the first tab of a server that plays (else the first with links), newest first. A film is one episode. Season packs ("S01:E01-61") are followed to their own page of episodes (two requests a pack) |
| `hosters` | the episode page: in each DoodStream and Mp4Upload tab, the link labelled like the current episode |
| `videos` | that link's page has the player (`#player-embed iframe`), or a link to a page that has it; read by [`lib/extractors`](../../lib/extractors) (DoodStream asked at playmogo.com) |

The manifest lists the site and every host in `kuro_extractors::HOSTS`; a test checks that it does.

## Not ported

- The servers no extractor reads: VOE, Filemoon, PlayN, ToastP, VidGuard (listeamed.net).
- The preferences (preferred and excluded servers).

## Test

```
cargo test -p kuro-animetoast
```

The fixtures were saved from the live site on 2026-10-25. Checked live the same day through the app's
runtime: every op and an Mp4Upload video (1080p); a film's DoodStream page (dooodster.com) was checked by hand to
answer at playmogo.com.
