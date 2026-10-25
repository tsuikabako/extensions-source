# Yokai

Yokai (<https://yokai-team.blogspot.com>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). A
ZeistManga (Blogger) site: the ops, URLs and parsers are the shared [`lib/zeistmanga`](../../lib/zeistmanga) crate, and
`src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ar/yokai` extension (`Yokai`) and its
`zeistmanga` theme (`lib-multisrc/zeistmanga`, Apache-2.0). The extension sets no id, so the source id is the Kotlin's
generated one: `MD5("yokai/ar/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`4337506350787725909`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests |
| `request-headers` | the page-image headers: the site as Referer |
| `popular` | `GET /`: the home page's popular list (one page) |
| `latest` | `GET /feeds/posts/default/-/Series?alt=json&orderby=published&max-results=21&start-index=N` (20 a page; a 21st asks for a next page) |
| `search` | the same feed with `q=label:Series <words>` |
| `details` | the title page: title, summary and other names, genres, author and artist, status, cover |
| `chapters` | the chapter feed the title page names (`/feeds/posts/summary/-/<label>`), 150 posts a request, newest first, with dates |
| `pages` | the chapter page's images, else those of the post's feed entry |

The Kotlin's chapter list: dates from the posts' last update, `Chapter N` renamed `الفصل N` with its number, and the title page's download links (`div#download`) after the feed's chapters, each address once.

Titles and chapters are stored by their path on the blog (`/2023/05/<slug>.html`).

## Not ported

- The search filters (status, type, language, genres).
- Related titles and opening a title from a pasted link (the Kotlin's deep link).

## Test

```
cargo test -p kuro-zeistmanga
cargo test -p kuro-yokai
```

The theme's fixtures (in `lib/zeistmanga/fixtures`) were saved from the live ZeistManga sites on 2026-10-25.
