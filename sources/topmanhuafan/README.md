# TopManhua.fan

TopManhua.fan (<https://www.topmanhua.fan>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`,
manga ops). A Madara site: the ops, URLs and parsers are the shared [`lib/madara`](../../lib/madara) crate, and
`src/lib.rs` holds only the site and the ABI glue.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/topmanhuafan` extension and its `madara` theme
(`lib-multisrc/madara`, Apache-2.0). The source id is the Kotlin one: `MD5("topmanhua.fan/en/1")` taken as its first 8
bytes, big-endian, with the top bit cleared: `6293890187960663957`.

## Ops

The Kotlin extension uses the theme's ajax lists (`admin-ajax.php`), but the site answers those with 404. This port uses
the theme's older paged layout (the Kotlin `MadaraNoAjax`), which the site serves.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `request-headers` | the page-image headers: the site as Referer (the image hosts answer 403 without it) |
| `popular` | `GET /manhua/page/N?m_orderby=views`; a next page when the page has the "older" link |
| `latest` | `GET /manhua/page/N?m_orderby=latest` |
| `search` | `GET /?s=...&post_type=wp-manga`, first page only: the site returns nearly the same results on every page |
| `details` | the title page: title, summary, genres and type, status, cover |
| `chapters` | the same page, newest first, with dates (`MM/dd/yyyy`) |
| `pages` | the chapter page: the `div.page-break img` addresses in order |

Addresses have no trailing slash: the site redirects `/manhua/` to plain `http`.

## Not ported

- The search filters (genre, sort).
- The chapter protector (encrypted page lists), the view counter and related titles (see `lib/madara`).
- Post ids as stored titles: titles are stored by path (`/manhua/<slug>`). A stored bare post id is fetched as `/?p=<id>`.

## Test

```
cd lib/madara && cargo test
cd sources/topmanhuafan && cargo test
```

The fixtures (in `lib/madara/fixtures/pages`) were saved from the live site on 2026-10-25.
