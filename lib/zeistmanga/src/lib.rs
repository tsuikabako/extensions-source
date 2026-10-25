//! The ZeistManga Blogger theme, shared by every ZeistManga site. Ported from the keiyoushi `lib-multisrc/zeistmanga`
//! theme (Apache-2.0, base version 14). A site is one `Site` value and one line, `kuro_guest::plugin!(SITE);`; this
//! crate holds the ops, the URLs and the parsers (`site.rs`).
//!
//! The lists and the chapter lists are Blogger's JSON feeds (`/feeds/posts/default/-/<label>?alt=json`); the title and
//! chapter pages are the blog's post pages. Left out: the filters and related titles. Titles and chapters are stored by
//! their path on the blog (`/2023/05/blue-lock.html`).

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// One ZeistManga site.
pub struct Site {
    /// The source id: `generate_id(name, lang, 1)`, the MD5 of `<name lower-case>/<lang>/1`, as the Kotlin extension has it.
    pub id: i64,
    pub name: &'static str,
    pub lang: &'static str,
    /// The blog root without a trailing slash (`https://example.blogspot.com`); every request goes there.
    pub base: &'static str,
    /// The cards of the home page's popular list (the Kotlin `popularMangaSelector`). `None` (the Kotlin
    /// `supportsLatest = false`): Popular is the newest titles and there is no Latest.
    pub popular: Option<&'static str>,
    /// The label of the chapter posts (the Kotlin `chapterCategory`, `Chapter` by default).
    pub chapter_label: &'static str,
    /// Without a chapter-feed script on the title page, the chapters are labelled with the title's name; `true` takes
    /// the label from the page's `[data-label]` instead.
    pub data_label: bool,
    pub details: Details,
    /// The elements that hold a chapter's images (the Kotlin `pageListSelector`).
    pub pages: &'static str,
    /// The Yokai chapter list: updated dates, `Chapter N` renamed `الفصل N` with its number, and the title page's
    /// download links added.
    pub yokai: bool,
    /// The least gap between two requests to the site.
    pub request_gap_ms: u64,
}

/// The selectors of a title page that differ between sites (the Kotlin `mangaDetailsSelector*`).
pub struct Details {
    /// The block holding the details.
    pub root: &'static str,
    pub genres: &'static str,
    /// The rows of the info table, each a heading and a value.
    pub info: &'static str,
    pub info_title: &'static str,
    pub info_value: &'static str,
}

impl Details {
    pub const DEFAULT: Details =
        Details { root: ".grid.gtc-235fr", genres: "div.mt-15 > a[rel=tag]", info: ".y6x11p", info_title: "strong", info_value: "span.dt" };
}

/// The Kotlin's defaults for a site that overrides nothing; a site sets `id`, `name`, `base` and what differs.
pub const DEFAULT: Site = Site {
    id: 0,
    name: "",
    lang: "ar",
    base: "",
    popular: Some("div.PopularPosts div.grid > figure"),
    chapter_label: "Chapter",
    data_label: false,
    details: Details::DEFAULT,
    pages: "div.check-box div.separator",
    yokai: false,
    // the Kotlin has no limit; two requests a second keep well under Blogger's
    request_gap_ms: 500,
};

impl Source for Site {
    fn info(&self) -> Info {
        Info { id: self.id, name: self.name, lang: self.lang, supports_latest: self.popular.is_some(), request_gap_ms: Some(self.request_gap_ms) }
    }

    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), format!("{}/", self.base))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let s = self;
        match op {
            "popular" if let Some(cards) = s.popular => to_value(site::parse_popular(s, cards, &s.get(host, &format!("{}/", s.base))?)),
            "popular" | "latest" => to_value(site::parse_list(&s.get(host, &site::list_url(s, input.page(), ""))?)?),
            "search" => to_value(site::parse_list(&s.get(host, &site::list_url(s, input.page(), input.query()))?)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(s, &s.get(host, &site::page(s, url)?)?)?;
                found.url = site::path_of(url);
                to_value(found)
            }
            "chapters" => {
                let html = s.get(host, &site::page(s, input.url("manga")?)?)?;
                let feed = site::chapter_feed(s, &html);
                let mut entries = Vec::new();
                loop {
                    let (total, page) = site::parse_feed(&s.get(host, &site::feed_range(&feed, entries.len() + 1))?)?;
                    let done = page.is_empty();
                    entries.extend(page);
                    if done || entries.len() >= total {
                        break;
                    }
                }
                let mut chapters = site::chapters(s, entries);
                if s.yokai {
                    site::add_downloads(&mut chapters, &html);
                }
                to_value(chapters)
            }
            "pages" => {
                let html = s.get(host, &site::page(s, input.url("chapter")?)?)?;
                let mut images = site::parse_pages(s, &html);
                // some sites fill the reader with a script: the images are in the post's own feed entry
                if images.is_empty()
                    && let Some(id) = site::post_id(&html)
                {
                    images = site::parse_post(s, &s.get(host, &format!("{}/feeds/posts/default/{id}?alt=json", s.base))?)?;
                }
                if images.is_empty() {
                    return Err(format!("this chapter has no images on {} (it may be locked)", s.name));
                }
                to_value(images.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

impl Site {
    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        Request::get(url).header("Referer", format!("{}/", self.base)).text(host, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const LONER: Site = Site { id: 1, name: "Loner", base: "https://loner-tl.blogspot.com", ..DEFAULT };
    const XSANO: Site = Site {
        id: 2,
        name: "XSano",
        base: "https://www.xsano-manga.com",
        popular: None,
        details: Details { root: "main", genres: "dl a[rel=tag]", info: "#extra-info dl", info_title: "dt", info_value: "dd" },
        pages: "#reader div.separator",
        ..DEFAULT
    };

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// Runs one op against a host that answers each request whose address contains a key with that fixture.
    fn call(site: &Site, op: &str, args: Value, routes: &[(&str, &str)]) -> (Res<Value>, Vec<Request>) {
        let routes: Vec<(String, String)> = routes.iter().map(|(k, f)| (k.to_string(), fixture(f))).collect();
        let host = TestHost::answering(move |request| match routes.iter().find(|(k, _)| request.url.contains(k.as_str())) {
            Some((_, body)) => (200, body.clone()),
            None => (404, String::new()),
        });
        let out = kuro_guest::invoke(site, &host, op, &json!({ "args": args }).to_string());
        (out.map(|o| serde_json::from_str(&o).unwrap()), host.sent())
    }

    #[test]
    fn info_says_whether_there_is_a_latest_list() {
        let (info, _) = call(&XSANO, "info", json!({}), &[]);
        assert_eq!((info.unwrap()["supportsLatest"].as_bool()), Some(false));
        let (headers, _) = call(&LONER, "request-headers", json!({}), &[]);
        assert_eq!(headers.unwrap(), json!([["Referer", "https://loner-tl.blogspot.com/"]]));
        assert_eq!(call(&LONER, "hosters", json!({}), &[]).0, Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn popular_is_the_home_page_list_or_the_newest_feed() {
        let (page, sent) = call(&LONER, "popular", json!({ "page": 1 }), &[("blogspot.com/", "loner/home.html")]);
        let page = page.unwrap();
        assert_eq!(sent[0].url, "https://loner-tl.blogspot.com/");
        let first = &page["mangas"][0];
        assert!(!first["title"].as_str().unwrap().is_empty() && first["url"].as_str().unwrap().ends_with(".html"), "{first}");
        assert!(first["thumbnailUrl"].as_str().unwrap().starts_with("https://"));

        let (page, sent) = call(&XSANO, "popular", json!({ "page": 2 }), &[("/feeds/", "xsano/feed.json")]);
        assert_eq!(sent[0].url, "https://www.xsano-manga.com/feeds/posts/default/-/Series?alt=json&orderby=published&max-results=21&start-index=21");
        let page = page.unwrap();
        assert_eq!((page["mangas"].as_array().unwrap().len(), page["hasNextPage"].as_bool()), (8, Some(false)));
        assert_eq!(page["mangas"][0]["url"], "/2023/02/be-blues.html");
    }

    #[test]
    fn search_asks_the_feed_for_series_posts() {
        let (page, sent) = call(&LONER, "search", json!({ "page": 1, "query": "stage s" }), &[("/feeds/", "loner/feed.json")]);
        assert_eq!(sent[0].url, "https://loner-tl.blogspot.com/feeds/posts/default/-/Series?alt=json&q=label%3ASeries+stage+s&max-results=21&start-index=1");
        assert_eq!(page.unwrap()["mangas"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn details_read_the_title_page() {
        let manga = json!({ "manga": { "url": "/2022/09/my-dearest-self-with-malice-aforethought.html" } });
        let (found, _) = call(&LONER, "details", manga, &[("/2022/09/", "loner/title.html")]);
        let found = found.unwrap();
        assert_eq!(found["title"], "My Dearest Self With Malice Aforethought");
        assert!(found["description"].as_str().unwrap().len() > 50 && found["genre"].as_str().unwrap().contains(", "), "{found}");

        let (found, _) =
            call(&XSANO, "details", json!({ "manga": { "url": "https://www.xsano-manga.com/2023/02/be-blues.html" } }), &[("/2023/02/", "xsano/title.html")]);
        let found = found.unwrap();
        assert_eq!((found["title"].as_str(), found["url"].as_str(), found["status"].as_i64()), (Some("BE BLUES"), Some("/2023/02/be-blues.html"), Some(1)));
        // the site fills its info table (author, artist) with a script
        assert!(found["author"].is_null() && found["genre"].as_str().unwrap().contains("Sports"), "{found}");
    }

    #[test]
    fn chapters_come_from_the_feed_the_title_page_names() {
        // the title page runs `clwd.run('<label>')`: the chapter posts are labelled Chapter and <label>
        let manga = json!({ "manga": { "url": "/2022/09/my-dearest-self-with-malice-aforethought.html" } });
        let (chapters, sent) = call(&LONER, "chapters", manga, &[("/feeds/", "loner/chapters.json"), ("/2022/09/", "loner/title.html")]);
        let chapters = chapters.unwrap();
        assert_eq!(
            sent[1].url,
            "https://loner-tl.blogspot.com/feeds/posts/summary/-/Chapter/My%20Dearest%20Self%20With%20Malice%20Aforethought?alt=json&start-index=1&max-results=150"
        );
        assert_eq!(sent.len(), 2);
        assert_eq!(chapters.as_array().unwrap().len(), 98);
        assert_eq!(chapters[1]["name"], "الفصل 97");
        assert_eq!(chapters[1]["url"], "/2023/04/my-dearest-self-with-malice-chapter-97.html");
        assert_eq!(chapters[1]["dateUpload"], 1680387120004_i64);
    }

    #[test]
    fn an_old_title_page_names_its_feed_in_a_script_address() {
        let ai = Site { chapter_label: "فصل", ..LONER };
        let (chapters, sent) = call(
            &ai,
            "chapters",
            json!({ "manga": { "url": "/2020/09/usogui.html" } }),
            &[("/feeds/", "ailand/chapters.json"), ("/2020/09/", "ailand/title.html")],
        );
        assert_eq!(sent[1].url, "https://loner-tl.blogspot.com/feeds/posts/summary/-/Usogui?alt=json&start-index=1&max-results=150");
        // the feed says 159 posts: a second page from 151, which the fixture answers again
        assert_eq!(sent[2].url, "https://loner-tl.blogspot.com/feeds/posts/summary/-/Usogui?alt=json&start-index=151&max-results=150");
        // the title's own post is in its label but is not a chapter
        assert_eq!(chapters.unwrap().as_array().unwrap().len(), 298);
    }

    #[test]
    fn yokai_adds_the_download_links_and_numbers_its_chapters() {
        let yokai = Site { yokai: true, ..LONER };
        let manga = json!({ "manga": { "url": "/2023/09/hunter-x-hunter.html" } });
        let (chapters, _) = call(&yokai, "chapters", manga, &[("/feeds/", "yokai/chapters.json"), ("/2023/09/", "yokai/title.html")]);
        let chapters = chapters.unwrap();
        let chapters = chapters.as_array().unwrap();
        assert_eq!((chapters[0]["name"].as_str(), chapters[0]["chapterNumber"].as_f64()), (Some("الفصل 410"), Some(410.0)));
        assert!(chapters.len() > 172, "{}", chapters.len());
        assert!(chapters.iter().any(|c| c["url"] == "/2023/09/chapter-11.html"));
    }

    #[test]
    fn pages_are_the_reader_images_or_the_post_feed_images() {
        let chapter = json!({ "chapter": { "url": "/2023/08/82.html" } });
        let (pages, _) = call(&XSANO, "pages", chapter, &[("/2023/08/", "xsano/chapter.html")]);
        let pages = pages.unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 20);
        assert!(pages[0]["imageUrl"].as_str().unwrap().starts_with("https://blogger.googleusercontent.com/"));

        // Orca Manga: a page without the images, then the post's feed entry
        let chapter = json!({ "chapter": { "url": "/2021/06/chapter-332.html" } });
        let (pages, sent) = call(&LONER, "pages", chapter, &[("/feeds/", "orca/post.json"), ("/2021/06/", "orca/chapter.html")]);
        assert_eq!(sent[1].url, "https://loner-tl.blogspot.com/feeds/posts/default/4873607168521498898?alt=json");
        assert_eq!(pages.unwrap().as_array().unwrap().len(), 8);

        let (empty, _) = call(&LONER, "pages", json!({ "chapter": { "url": "/x.html" } }), &[("/x.html", "loner/home.html")]);
        assert!(empty.unwrap_err().starts_with("this chapter has no images"));
    }
}
