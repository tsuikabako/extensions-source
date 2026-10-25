# AllAnime

AllAnime (<https://allmanga.to>, API <https://api.allanime.day>) as an anime source plug-in for Kurobayomi: anime
subbed in English.

**Videos do not play today.** The API now answers an episode's server list only to the site's own app: the streams
query comes back with the error `AA_CRYPTO_MISSING` (the site signs the query and decrypts the answer in the browser,
with keys that change every few days). The plug-in shows that error for `hosters` and `videos`. The lists, details and
episodes work.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the `en/allanime` Aniyomi extension (Apache-2.0,
`AllAnime.kt`, `AllAnimeExtractor.kt`). The source id is the Kotlin one: the extension sets none, so it is
`MD5("allanime/en/1")` taken as its first 8 bytes, big-endian, with the top bit cleared: `4709139914729853090`.
Stored addresses are the Kotlin ones, so backups match: a title is `<id><&sep><slugTime><&sep><slug>`, an episode is
the whole streams request as JSON (`{"variables":{"showId",...},"query":"..."}`, the query text as the Kotlin writes it).
The icon is the AllManga icon of the keiyoushi `en/allanime` extension (the anime extension's reference has none).

## Ops

Every op is one GraphQL `POST /api` with `Origin` and `Referer` of the site (a GET meets Cloudflare's challenge, a
POST does not).

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest offered |
| `popular` | `queryPopular` (anime, last 7 days), 26 a page |
| `latest` | `shows` without text (the site's update order), subbed, 26 a page |
| `search` | `shows` with the text, subbed |
| `details` | `show`: synopsis with type, season and score (as the Kotlin), genres, status, first studio, cover |
| `episodes` | `show.availableEpisodesDetail.sub`, newest first, "Episode N (sub)" |
| `hosters` | `episode.sourceUrls`: the site's own servers (Default, Ac, Ak, Kir, Luf-mp4, Si-Hls, S-mp4, Ac-Hls, the Kotlin's default set; their `--` addresses decoded: hex bytes xor 56), direct player files (`type: player`), and embeds on a host in `kuro_extractors::HOSTS`; highest priority first |
| `videos` | own server: `GET https://allanime.day/apivtwo/clock.json?...`, its mp4 files and HLS playlists (one video each, with subtitles; HLS with `Origin`/`Referer` of allanime.day). Player file: the file itself. Embed: [`lib/extractors`](../../lib/extractors) |

The manifest lists the API, `allanime.day` (the own servers' links) and every host in `kuro_extractors::HOSTS`; a test
checks that it does.

## Not ported

- The filters and the settings (title style: romaji; sub or dub: sub; preferred quality and server; host switches).
- Servers on hosts no extractor reads: Vidstreaming, Ok.ru (until `lib/extractors` reads it), StreamLare, Filemoon,
  StreamWish, and the Rab, Uv-mp4 and Pn-Hls own servers (off by default in the Kotlin).
- DASH and Crunchyroll-iframe links of the own servers.
- The `getVersion` lookup of the player host: the site no longer gives it (`{"data":"0.195"}`); the host is
  `https://allanime.day`, as the later MKissa extension uses.

## Test

```
cargo test -p kuro-allanime
```

The list, details, episode and `AA_CRYPTO_MISSING` fixtures are live answers of 2026-10-25. Checked live the same day
through the app's runtime: popular, latest, search, details and episodes work; `hosters` gives the error above.
**Not checked live, to confirm in the app if the API ever answers again:** the server list and the videos.
`constructed-streams.json` and `constructed-clock.json` are written from the Kotlin's DTOs (`sourceUrls`, `links`), not
saved from the site.
