# AllAnime is parked

Its lists, details and episodes work, but no episode can play: since October 2026 the API answers the streams query
with `AA_CRYPTO_MISSING`. The site now signs that query and decrypts the answer in the browser, with keys that change
every few days (the newer MKissa extension, on the same API, runs a hidden WebView for it). A sandboxed plug-in cannot do
that, so the source is not offered. The anime side of the site seems to have moved to mkissa.to.

The code and README are kept. Move this folder back to `sources/allanime` if the API answers the streams query again,
then re-check the server code against the live answers (its stream fixtures were built from the Kotlin's data shapes).
