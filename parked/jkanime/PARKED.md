# JKAnime is parked

The plug-in builds, but the site cannot be reached. jkanime.net answers every request with Cloudflare's browser check,
and on 2026-11-02 the check did not clear in the app's hidden page (WebKitGTK): it shows a "Verify you are human" box
that needs a click, and the hidden page was dropped after 25 s ("the site's browser check did not clear"). The app does
not show such a check to the person, and does not solve it either, so the source is not offered.

AnimePahe and Mangakakalot sit behind the same kind of check, but theirs clears by itself in the hidden page. Move this
folder back to `sources/jkanime` if JKAnime's check does the same again (open it in the app and watch the log for
`a hidden page loaded uri="https://jkanime.net/" ... ok=true`). Then replace the hand-made fixtures with saved pages (see
`fixtures/README.md`) and re-check the parsers against them, above all the episodes API and the Nozomi key, which were
written from the Kotlin alone.
