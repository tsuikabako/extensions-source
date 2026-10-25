# Contributing

Fixes for broken sources and new sources are welcome. This page covers what is where in this repository, how to try a
source in the app, and what a pull request needs. How to write a source (recipes, Madara sites, plug-ins on the toolkit)
is explained step by step at **https://tsuikabako.github.io/make/**.

## Before you start

- **Bug reports:** open an issue with the source's name, what you did and what happened.
- **New sources:** open an issue first, so two people do not port the same site.
- **Ports only:** every source here is a port of an open-source Aniyomi, Mihon or keiyoushi extension under
  Apache-2.0. Name the original in the source's README. Do not copy code under another licence.

## Three ways to write a source

| | Text recipe | Theme site | WASM plug-in |
|---|---|---|---|
| File | `recipe.toml` | a `Site` value in a tiny crate | a Rust crate built to `.wasm` |
| Good for | simple HTML sites: lists, pages, links | Madara WordPress sites | APIs, POST requests, relative dates, video hosts |
| Runs code | no, only CSS selectors and a few rules | yes, the shared `lib/madara` | yes, in the app's sandbox, with no file or system access |
| Guide | [Recipes](https://tsuikabako.github.io/make/recipe) | [Madara sites](https://tsuikabako.github.io/make/madara) | [Plug-ins](https://tsuikabako.github.io/make/plugin) |

Try a recipe first. Use a plug-in when the site needs real logic.

## Repository layout

```
sources/<id>/         one folder per source; <id> is lower-case letters, digits, '.', '_', '-'
  recipe.toml         a recipe: id, name, lang, kind, version, base_urls, selectors
  manifest.toml       or a plug-in: id, name, lang, kind, version, hosts
  Cargo.toml, src/    the plug-in crate, a member of the workspace (the root Cargo.toml)
  fixtures/           saved pages and responses the tests read
  icon.png            the site's icon, square PNG
  health.toml         the page the health check fetches to see the site is up (url, expect, cloudflare)
  README.md           what it does, what was left out, the original extension
parked/<id>/          sources kept but not offered; never read by the tools
lib/guest/            the toolkit every plug-in is built on: the glue to the app, requests, the JSON shapes, test host
lib/<theme>/          code shared by the plug-ins of one site theme (lib/madara: every Madara WordPress site)
wit/                  the contract with the app (kuro-source.wit), one copy for every plug-in
template/example/     a complete example plug-in to copy for a new source
Cargo.toml, Cargo.lock the workspace: one set of dependency versions and one build for everything
status.toml           sources marked blocked or broken by hand (overrides health.toml)
health.toml           written by tools/check-health.py: sources found broken (do not edit)
health-state.json     the failures in a row per source, for the next check
tools/check-health.py checks every source's site; the publish workflow runs it every six hours
tools/build.sh        builds every plug-in into dist/
tools/make-index.py   copies recipes and icons into dist/ and writes index.min.json
output/README.md      the README of the published repository
dist/, index.min.json built, never committed: CI publishes them to tsuikabako/extensions
```

## Add a recipe

1. Create `sources/<id>/recipe.toml`. Its `id` must be the folder name. It also needs `name`, `lang`, `kind`
   (`manga` or `anime`) and `version` (default 1). The keys are in
   [source-recipes.md](https://github.com/kurobayomi/kurobayomi/blob/main/docs/source-recipes.md).
2. Save a few real pages from the site into `fixtures/` and point the recipe's `[tests]` section at them.
3. Check it from a clone of the [app repository](https://github.com/kurobayomi/kurobayomi):

   ```bash
   cargo run -p kuro-source-scrape --example check_recipe -- /path/to/recipe.toml /path/to/fixtures/
   ```

   Add `--live` to run it against the real site too.
4. Add `icon.png` and a `README.md`.

## Add a plug-in

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked wasm-component-ld
```

1. Copy the example and rename the package in `Cargo.toml` to `kuro-<id>`:

   ```bash
   cp -r template/example sources/<id>
   ```

2. Set `manifest.toml` (`id` is the folder name; `hosts` are the only hosts the plug-in requests itself, since images
   and videos load in the app), then write the site's operations in `src/lib.rs`. The example is a whole manga source
   with every part to change marked `CHANGE`; [the plug-in guide](https://tsuikabako.github.io/make/plugin) walks
   through the toolkit (`lib/guest`). Existing sources are good models: `sources/mangadex` for a JSON API,
   `sources/mangapill` for HTML, `sources/animepahe` for HLS video.
3. Save responses into `fixtures/` and test the operations against them:

   ```bash
   cargo test -p kuro-<id>
   ```

4. Add `icon.png` (square, about 192 px) and a `README.md`.

A built plug-in must be under 1 MiB. `tools/build.sh` fails when it is bigger, and also when it imports anything but the
app's host interface. Build for `wasm32-unknown-unknown` through the script, never for `wasm32-wasip2`: the sandbox has
no WASI, so a wasip2 build cannot start.

## Add a Madara site

Many manga sites run the Madara WordPress theme. Its ops, URLs and parsers live once in `lib/madara`, so a Madara site is
only its settings: copy `sources/topmanhuafan`, rename the folder and package, and set the `Site` in `src/lib.rs` (id,
name, lang, address, title folder, and how the site lists titles and chapters). [The Madara guide](https://tsuikabako.github.io/make/madara)
explains how to tell each setting. A fix to the theme goes in `lib/madara` and reaches every Madara site. GroupLe sites (Russian: Usagi, ReadManga, SelfManga) work the same way on `lib/grouple`.

## Fix a source

Make the change, then bump `version` in its `manifest.toml` or `recipe.toml`. The app downloads a source's file again
whenever its checksum in the index changes, so users get any rebuilt file; the version says that the source changed.

If a source stops working and you cannot fix it, mark it in `status.toml` so users see why:

```toml
example = "blocked"   # the site refuses the app for now (orange dot)
example = "broken"    # the source needs a fix (red dot)
```

Remove the line once it works again.

A scheduled workflow (`.github/workflows/health.yml`, every 6 hours) also checks each site by itself with
`tools/check-health.py` and marks a source broken after two failed checks in a row, active again at the next good one.
Cloudflare's browser check counts as "unknown", not as down: the app passes it, this runner cannot. Give a new source a
`health.toml` (`url`, `expect`, `cloudflare = true` when the site has that check, and `user_agent` when the site
answers only the User-Agent the source sends). The rules are tested with
`python3 tools/test-check-health.py`.

## Build the index

You do not need to: on each push to `main`, CI (`.github/workflows/publish.yml`) builds every source, writes the index and
publishes them to [tsuikabako/extensions](https://github.com/tsuikabako/extensions). To check a change locally first, run
both from the repository root:

```bash
tools/build.sh
```

```bash
python3 tools/make-index.py
```

`build.sh` builds each plug-in into `dist/<id>.wasm`. `make-index.py` copies recipes and icons into `dist/`, checks every
source, removes files no source names, and writes `index.min.json` with each file's checksum.

Do not commit `dist/` or `index.min.json`; they are ignored here and written by CI. Never edit an index by hand: the
checksums in it must match the files, or the app refuses the download.

## Try it in the app

Build an index that points at a throwaway branch of your fork, and push the built files there only:

```bash
tools/build.sh
python3 tools/make-index.py --base-url https://raw.githubusercontent.com/<you>/extensions-source/<test-branch>
git switch -c <test-branch> && git add -f dist index.min.json && git commit -m test && git push -u origin <test-branch>
```

Then add `https://raw.githubusercontent.com/<you>/extensions-source/<test-branch>/index.min.json` in Kurobayomi
(**Settings → Sources and tracking → Repositories → Add**). Open the pull request from a branch without the built
files.

A recipe can also be tried without a repository: copy `recipe.toml` to `~/.local/share/kurobayomi/scrapers/<id>.toml`
(`kurobayomi-nightly` for the nightly app) and restart the app.

## Pull request checklist

- [ ] The source's README names the extension it was ported from and what was left out.
- [ ] `version` is bumped (for a fix) or set to 1 (for a new source).
- [ ] Tests pass against the saved fixtures.
- [ ] `tools/build.sh` and `python3 tools/make-index.py` run without errors locally; `dist/` and `index.min.json` are
      not in the pull request.
- [ ] For a new source, the README's source table has a row for it.

By contributing you agree that your work is licensed under the Apache License 2.0, like the rest of this repository.
