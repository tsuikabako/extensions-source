# Mangasushi

Mangasushi (<https://mangasushi.org>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops). A Madara site:
the ops, URLs and parsers are the shared [`lib/madara`](../../lib/madara) crate, and `src/lib.rs` holds only the site.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/mangasushi` extension and its `madara` theme
(`lib-multisrc/madara`, Apache-2.0). The source id is the Kotlin one: `MD5("mangasushi/en/1")` taken as its first 8
bytes, big-endian, with the top bit cleared: `3606721916149050760`.

## How it reads the site

Titles from the theme's ajax lists (`admin-ajax.php`), chapters from a POST to `<title page>/ajax/chapters/`, 1 s between requests, the site as Referer for page images. What is
left out (filters, encrypted page lists, the view counter, related titles) is listed in `lib/madara/src/lib.rs`.

## Test

```
cd lib/madara && cargo test
cd sources/mangasushi && cargo test
```

Checked live on 2026-10-25: popular, latest, search, details, chapters and pages.
