# LeerCapitulo

LeerCapitulo (<https://www.leercapitulo.co>) as a Spanish manga source plug-in for Kurobayomi (ABI 1.5,
`wit/kuro-source.wit`, manga ops).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `es/leercapitulo` extension (Apache-2.0). The selectors
follow it. The source id is the Kotlin one: `MD5("leercapitulo/es/1")` taken as its first 8 bytes, big-endian, with the top
bit cleared: `5621901596522308538`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 3 s between requests (the Kotlin's rate limit) |
| `popular` | `GET /`: the "Tendencias" slider (one page) |
| `latest` | `GET /`: the latest releases (one page) |
| `search` | `GET /manga/?q=...&page=N`; there is a next page when the page has a `rel=next` link |
| `details` | `GET /manga/{id}/{slug}/`: title, synopsis and alternative names, genres and themes, author, artist, status, cover |
| `chapters` | the same page: every chapter row, newest first, with its date |
| `pages` | `GET /leer/{id}/{slug}/{n}/`: the `#lcPages img[data-src]` addresses in order |

Page images load without a Referer, so the plug-in keeps the app's default image headers.

## Not ported

- The search filters (genre, theme, type, status, order).
- Opening a title from a pasted link (the Kotlin's deep link).

## Test

```
cd sources/leercapitulo && cargo test
```

The fixtures were saved from the live site on 2026-10-25 and trimmed to a few cards and chapters. Checked live the same
day: popular, latest, search, details, chapters and pages.
