# Okanime

Okanime (<https://ww3.okanime.xyz>) as an anime source plug-in for Kurobayomi: anime with Arabic subtitles.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `ar/okanime` extension (Apache-2.0). The source id is the
MD5 of `okanime/ar/1`, as the Kotlin extension has it: `2275740574829569178`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; 0.5 s between requests |
| `popular` | `GET /top-anime?page=N`: the `div.anime-card` cards, more pages while the pager's last item is a link |
| `latest` | `GET /recently-uploaded-episodes?page=N`, episode cards that link their title |
| `search` | `GET /search?q=...&page=N`, the same cards (no text: the popular list) |
| `details` | the title page: title, cover, summary, genres, status |
| `episodes` | the title page's episode buttons (all on one page), newest first: "الحلقة 3" |
| `hosters` | the episode page: one server per Mp4upload or Uqload link (the extractors' hosts), named with its quality ("Mp4upload (HD)"); a Uqload `/e/<id>` link (a click-to-play page) becomes `/embed-<id>.html` |
| `videos` | the host's player page, read by [`lib/extractors`](../../lib/extractors) |

The manifest lists the site and every host in `kuro_extractors::HOSTS`; a test checks that it does.

## Not ported

- The servers VOE, Vidbom, VK, MEGA, share4max and Sendvid (no extractor reads them; VOE moves to new domains all the time).
- The preferences (preferred quality, enabled hosts): each server is named with its quality instead.
- The `id:` search and the URL intent.

## Test

```
cargo test -p kuro-okanime
```

The fixtures were saved from the live site on 2026-10-25 (scripts removed). The site was redesigned since the Kotlin
(new list addresses, title page and episode list); the parsers follow the live pages. Checked live the same day through
the app's runtime: every op, and a video from each of the five servers of an episode (four Mp4upload, one Uqload). Most
episodes offer Mp4upload; a few (the newest Re:Zero ones) offer only VK, MEGA and share4max, and then no server.
