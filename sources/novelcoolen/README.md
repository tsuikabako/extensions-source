# NovelCool (English)

NovelCool in English (<https://www.novelcool.com>) as a manga source plug-in for Kurobayomi (ABI 1.5,
`wit/kuro-source.wit`, manga ops). The ops, URLs and parsers are the shared [`lib/novelcool`](../../lib/novelcool) crate,
and `src/lib.rs` holds only the language.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `all/novelcool` extension (Apache-2.0), which makes one
source per language. The source id is the Kotlin one: `MD5("novelcool/en/1")` taken as its first 8 bytes, big-endian,
with the top bit cleared: `7570101320206504111`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests (the Kotlin's rate limit) |
| `popular` | `POST https://api.novelcool.com/elite/hot/` (the app API, manga only, 20 a page); titles the site marks for adults only with "Show adult" on |
| `latest` | `POST https://api.novelcool.com/elite/latest/`, the same way |
| `search` | `POST https://api.novelcool.com/book/search/` with the keyword, the same way |
| `details` | `GET /novel/{slug}.html`: title, author, summary, genres, status, cover |
| `chapters` | the same page (names and dates), then the newest chapter's reader page (its chapter menu gives the addresses), newest first |
| `pages` | `GET /chapter/{name}/{id}-10-1.html` and the chapter's other reader pages, ten images each |

The title page links its chapters through ad redirector domains (the Kotlin follows them to partner reader sites); this
port reads the same chapters on the site itself, so it only reaches the hosts in `manifest.toml`. Page images load
without a Referer, so the plug-in keeps the app's default image headers.

## Not ported

- Novels: the site hosts them too; the source offers manga only, as the Kotlin does.
- The Kotlin's HTML lists and its filters (author, genres, status, rating), which it uses only with the app API turned off.
- Opening a title from a pasted link (the Kotlin's deep link).

## Test

```
cd lib/novelcool && cargo test
cd sources/novelcoolen && cargo test
```

Checked live on 2026-10-25: popular, latest, search, details, chapters and pages.
