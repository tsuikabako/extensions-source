# InkStory

InkStory (<https://inkstory.net>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops),
Russian only.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/inkstory` extension and the `inkstory` theme it
builds on (`lib-multisrc/inkstory`, Apache-2.0). The server function ids and inputs (keiyoushi commit `a045a383`,
2026-11-05), the headers, the title, chapter and page rules and the image key follow it. The source id is the Kotlin
one: `MD5("inkstory/ru/2")` (the extension's `versionId = 2`) taken as its first 8 bytes, big-endian, with the top bit
cleared: `2026387314200968454`. Manga urls are the book slugs and chapter urls the chapter ids, as in the Kotlin, so
Aniyomi backups match.

## Ops

The site serves its data through its own server functions (TanStack Start):
`GET https://inkstory.net/_serverFn/<id>?payload=<seroval tree>` with `Accept: application/json` and `x-tsr-serverFn: true`.
The input and the answer are seroval node trees (`src/seroval.rs`); the function ids are the Kotlin's. Latest still reads
the shared API's update feed (`https://api.inuko.me/v2`, the Kotlin's `User-Agent` and `X-Service-Name: inkstory`), which
answered without a check on 2026-11-21.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest |
| `popular` | the catalogue function, `sort: viewsCount, orderBy: desc`, 40 a page; the next page from its `total` |
| `latest` | `GET /chapter-update-feed?onlyBorderChapters=true` on the API, the books of the latest chapter updates |
| `search` | the catalogue function with `search`, most viewed first. No filters |
| `details` | the book function (`slug`, `createView: false`): summary, rating, views, likes, bookmarks and links in the description; labels and formats as genres; authors and artists; status (ongoing, done, frozen). A missing book is "not found" |
| `chapters` | the book function for the book id, then the chapters function: every branch, newest first, named as the Kotlin does ("Том 1 Глава 84"); a chapter's teams are the scanlator |
| `pages` | the chapter function (`id`, `createView: false`): the page images in order |
| `unscramble` | ABI 1.5 image hook: an `x` page image is XOR-ciphered with a fixed key; it is read back when its bytes do not start like an image and the deciphered ones do (JPEG, PNG, GIF, WebP, AVIF). Anything else is shown as fetched |

**The site's check.** Since November 2026 inkstory.net answers every request from a plain client, server functions
included, with an Anubis proof-of-work page (HTTP 200, "Making sure you're not a bot!"). A browser clears it by itself.
The app sends such a request again from its hidden browser page, where it clears, so the source needs an app newer than
0.4.0 (the first that knows Anubis); on an older app every op but Latest fails. Whether the site shows the check in
Russia: unverified.

Covers and page images (`static.inuko.me`) load in the app and need no headers. The site has no adult (`EROTIC`) titles,
so the app's adult switch changes nothing here.

## Not ported

- The filters (genres, strict labels, status, country, content status, format, rating, year and chapter-count ranges,
  sort order) and the deep link.
- The settings: the branch mode ("latest version of a chapter" and "preferred team" instead of every branch) and hiding
  chapters newer than 4 days. The plug-in always lists every branch, the Kotlin's default.

## Tests

`cargo test` reads the fixtures. The `fn-*.json` files are the site's server function answers, saved untrimmed on
2026-11-21 through a WebView2 page (curl gets the Anubis check): a catalogue page, a search, a book, a missing book, a
chapter list and a chapter. `feed.json` (the update feed, 2026-10-23) and the first 64 bytes of a ciphered page image
(`page-start.bin`) are older and trimmed.
