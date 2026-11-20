# Comix

Comix (<https://comix.to>, mirror `comix.ws`) as a manga source plug-in for Kurobayomi: manga, manhwa and manhua in
English from many groups.

**It needs an app that reads pages for sources** (the app's `docs/source-abi.md`, "Page reads"): the plug-in imports
the app's `pages` interface and does not load in an app without it.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/comix` extension (Apache-2.0, version 65). The
source id is the Kotlin one, `MD5("comix/en/1")` as its first 8 bytes with the top bit cleared: `7537715367149829912`,
the id the first plug-in had, so library entries still match. Stored addresses are the Kotlin ones: `/<id>-<slug>` for
a title, `title/<id>-<slug>/<chapter id>-chapter-<n>` for a chapter.

## Why it reads pages

The site serves its lists, chapter lists and page lists only through API calls that its own script signs, with keys
that change at every build of the site, inside code written to be unreadable. The first plug-in (`parked/comix` in this repository's history)
signed the calls itself with keys captured from the site by hand; it died at the site's next build. The Kotlin
extension stopped signing too: it loads the real page in a WebView and reads what the page shows. This source does the
same through the app: `PageRead` (the toolkit's `pages`) has the app open an address in its hidden browser page, wait,
scroll and hand back the HTML of the elements named. The source passes no script and clicks nothing.

## Ops

| Op | What it asks |
|---|---|
| `info`, `create-sources` | the descriptor; Latest offered; 200 ms between requests |
| `popular` | page read of `/browse?sort=score:desc&page=N`: wait for a row or the "nothing matches" note, read `div.list-grid` and the pager |
| `latest` | the same with `sort=chapter_updated_at:desc` |
| `search` | the same with `q=<text>&sort=relevance:desc` (an empty search is `popular`) |
| `details` | a plain `GET /title/<id>-<slug>`: the page carries the details in `script#initial-data`. If that request is refused, the same script is read through a page read |
| `chapters` | page reads of `/title/<id>-<slug>?page=N` (20 chapters a page) until the pager has no next page; newest first; one entry per group that uploaded a chapter, as the site lists them |
| `pages` | page read of the chapter's reader: every page slot scrolled into view, a wait until each has its picture, then the slots are read (55 s limit). A chapter whose slots did not all fill is an error, not a short chapter |
| `request-headers` | `Accept: */*` only: the image hosts answer 403 to the site's Referer (checked) |
| `unscramble` | the page images the site hands out XOR-encoded or cut into a shuffled 5x5 grid are restored (`descramble.rs`, unchanged from the first plug-in; the image hosts say so in response headers) |

The manifest lists `comix.to` and `challenges.cloudflare.com`, so the page may ask nothing else: the site's analytics
and font hosts are refused and the pages still fill. The page images are on hosts that change (`447.*.site` on
2026-11-20) and load in the app.

The HTML is read with a small reader of its own (`tags.rs`), not the `scraper` crate: with the image decoder that
`unscramble` needs, `scraper` takes the plug-in over the 1 MiB limit (1,183,441 bytes against 682,406).

## The site's security check

The site puts a "Security check" (Cloudflare Turnstile, in a `wafcap` dialog) in front of a chapter list for a
browser it has not seen. It loads from `challenges.cloudflare.com`. With that host in the manifest the check runs in
the app's hidden page as it does in any browser and clears by itself, as Cloudflare's browser check does for
AnimePahe; the first chapter page then takes about 6 s instead of 2. Nothing answers the check for the person: if
it ever asks for a click, the wait is not met and the op fails with the app's error, which names the page.

Version 3 listed `comix.to` alone. The check could not load, every chapter list failed ("nothing matched
ul.mchap-list > li.mchap-item in time"), and the source was parked for a few hours on 2026-11-20. Measured through
the app's Windows engine with a new profile: `comix.to` alone fails after its time; with both hosts the chapter
list comes in 6 to 7 s and the reader fills 50 of 50 pages in 15 s.

## Not ported

- The search filters and the settings (content rating, types, demographics, hidden groups, chapter deduplication,
  maximum chapter length): the site's defaults apply, which leave adult titles out.
- Related titles, deep links, the mirror.
- The Kotlin's real taps and its answer to the site's check.

## Test

```
cargo test -p kuro-comix
```

The fixtures are pages saved on 2026-11-20 by a headless browser (Edge) that loaded them as a visitor does: a browse
page, a title page and its second chapter page, a reader page. Checked that day:

- With the headless browser: the browse page shows 28 titles, a search by `q` filters, a search with no match shows
  `div.list-empty`, the chapter list follows `?page=N`, the reader gives a picture only to the slots near the window.
- Through the app's own Windows engine (WebView2) by the session that built the page reads: the browse page (28
  titles, 2.2 s), a chapter page (20 chapters, 1.9 s) and the reader with the steps above (50 of 50 pictures, 10.3 s).
- A page image downloads as a plain WebP without a Referer.
- The plug-in builds to 682,406 bytes and imports only the app's `host` and `pages` interfaces.

Not checked: this plug-in loaded in the running app (no app with page reads is released yet); the Linux engine; a
title with hundreds of chapters (one page read per 20 chapters, about 2 s each, against the time the app gives a
chapter list); a scrambled page image from today's site (the three images looked at were plain).
