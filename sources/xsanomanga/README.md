# XSano Manga

XSano Manga (<https://www.xsano-manga.com>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). A
ZeistManga (Blogger) site: the ops, URLs and parsers are the shared [`lib/zeistmanga`](../../lib/zeistmanga) crate, and
`src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/xsanomanga` extension (`XSanoManga`) and its
`zeistmanga` theme (`lib-multisrc/zeistmanga`, Apache-2.0). The extension sets no id, so the source id is the Kotlin's
generated one: `MD5("xsano manga/ar/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`6170348197605244575`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest; 500 ms between requests |
| `request-headers` | the page-image headers: the site as Referer |
| `popular` | `GET /feeds/posts/default/-/Series?alt=json&orderby=published&max-results=21&start-index=N` (the newest titles, 20 a page; a 21st asks for a next page) |
| `search` | the same feed with `q=label:Series <words>` |
| `details` | the title page: title, summary and other names, genres, author and artist, status, cover |
| `chapters` | the chapter feed the title page names (`/feeds/posts/summary/-/<label>`), 150 posts a request, newest first, with dates |
| `pages` | the chapter page's images, else those of the post's feed entry |

Its own title-page layout (`main`, a `dl` info list) and reader (`#reader`), as the Kotlin overrides.

No popular list (the Kotlin `supportsLatest = false`): Popular is the newest titles.

The site fills the author and artist with a script, so details have none.

Titles and chapters are stored by their path on the blog (`/2023/05/<slug>.html`).

## Not ported

- The search filters (status, type, language, genres).
- Related titles and opening a title from a pasted link (the Kotlin's deep link).

## Test

```
cargo test -p kuro-zeistmanga
cargo test -p kuro-xsanomanga
```

The theme's fixtures (in `lib/zeistmanga/fixtures`) were saved from the live ZeistManga sites on 2026-10-25.
