# Vostfree

Vostfree (<https://vostfree.ws>) as an anime source plug-in for Kurobayomi: anime in French, subbed (VOSTFR) and
dubbed (VF).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `fr/vostfree` extension (Apache-2.0). The source id is the
Kotlin one: `MD5("vostfree/fr/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`8596049504378147571`.

## Ops

The site sends every visitor of vostfree.ws to ipv4.vostfree.ws, so the source asks that host.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest (the Kotlin has none); 0.5 s between requests |
| `popular` | `GET /films-vf-vostfr/page/N/`: the films, as in the Kotlin; a next page when "Suivant" is a link |
| `search` | `POST /index.php?do=search` (the site's form, one page); an empty search lists `GET /animes-vostfr/page/N/` |
| `details` | the title page: title, cover, summary, genres |
| `episodes` | the same page: one episode per option of its episode selector, newest first; stored as `<title page>?episode=N` |
| `hosters` | the same page: the episode's server buttons (`div#buttons_N`) whose video an extractor reads; Uqload and Okru buttons hold a video id (`uqload.io/embed-<id>.html`, `ok.ru/videoembed/<id>`), the others an embed address |
| `videos` | the server's embed page, read by [`lib/extractors`](../../lib/extractors): Uqload, Okru, DoodStream (asked at playmogo.com), MixDrop |

The manifest lists the site's two hosts and every host in `kuro_extractors::HOSTS`; a test checks that it does.

## Not ported

- The search filters (genre, type) and the preferred-quality setting.
- The servers no extractor reads: Sibnet, Vudeo, VOE, Myvi, MyTV, Vfree, Upvid, Streamsb, NinjaStream and others.

## Test

```
cargo test -p kuro-vostfree
```

The fixtures were saved from the live site on 2026-10-25. Checked live the same day through the app's
runtime: every op and a MixDrop video (the first film); the Uqload embeds of two series episodes were checked by hand
(their playlists load). The Okru and DoodStream videos seen on the site were removed by their hosts.
