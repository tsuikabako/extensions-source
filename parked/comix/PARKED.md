# Comix is parked

The plug-in builds and its lists and details work, but no chapter list or chapter can load: comix.to answers the
chapter-list request of an automated browser with its "Security check" (a Cloudflare Turnstile box in a `wafcap` dialog,
`/@waf/challenge`). Seen on 2026-11-20, a few hours after the source was published: in the nightly app (Windows) the
chapter list ended with "Comix isn't working right now", and a headless browser on the same connection got the check
on every title, with a new profile too, while a normal browser was let through.

The app answers no check, and its page reads let the page ask only the source's own hosts, so the check cannot even
load. The Kotlin extension gets past it by answering the check with a token helper; that is not done here.

It can come back if the app learns to show such a check to the person in a window, so that they pass it themselves and
the hidden page keeps the pass. Then move this folder back to `sources/comix`, check the chapter list and a chapter in
the app, and put its row back in the README.

The first Comix plug-in, which signed the site's API calls with keys captured by hand, is in this repository's history
(`parked/comix` before 2026-11-20).
