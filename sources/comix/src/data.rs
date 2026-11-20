//! A title's details, from the data a title page carries in `<script id="initial-data">` (so a plain request has
//! them, without a browser). The shapes and the description are the ones of the keiyoushi extension (`Dto.kt`,
//! Apache-2.0), carried over from the first Comix plug-in.

use std::collections::BTreeMap;

use kuro_guest::Res;
use kuro_guest::wire::{Manga, status};
use serde::Deserialize;
use serde_json::Value;

use crate::page::BASE;

#[derive(Deserialize)]
struct Term {
    title: String,
}

#[derive(Deserialize, Default)]
struct Poster {
    small: Option<String>,
    medium: Option<String>,
    large: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Detail {
    hid: String,
    title: String,
    synopsis: Option<String>,
    #[serde(default, rename = "type")]
    kind: String,
    poster: Option<Poster>,
    #[serde(default)]
    status: String,
    #[serde(default = "safe")]
    content_rating: String,
    authors: Option<Vec<Term>>,
    #[serde(rename = "author")]
    author_old: Option<Vec<Term>>,
    artists: Option<Vec<Term>>,
    #[serde(rename = "artist")]
    artist_old: Option<Vec<Term>>,
    genres: Option<Vec<Term>>,
    #[serde(rename = "genre")]
    genre_old: Option<Vec<Term>>,
    demographics: Option<Vec<Term>>,
    #[serde(rename = "demographic")]
    demographic_old: Option<Vec<Term>>,
    #[serde(default)]
    rated_avg: f64,
    #[serde(default)]
    rated_count: i64,
    #[serde(default)]
    follows_total: i64,
    #[serde(default)]
    rank: i64,
    year: Option<i64>,
    original_language: Option<String>,
    links: Option<BTreeMap<String, Option<String>>>,
    url: Option<String>,
}

fn safe() -> String {
    "safe".to_owned()
}

fn titles(terms: &Option<Vec<Term>>, old: &Option<Vec<Term>>) -> Vec<String> {
    terms.as_ref().or(old.as_ref()).map(|terms| terms.iter().map(|t| t.title.clone()).collect()).unwrap_or_default()
}

impl Detail {
    /// `/x0ynk-villains-are-destined-to-die`: the site address after `/title`, as the Kotlin stores it.
    fn url(&self) -> String {
        match &self.url {
            Some(url) => url.split_once("/title").map_or(url.as_str(), |(_, rest)| rest).to_owned(),
            None => format!("/{}", self.hid),
        }
    }

    fn cover(&self) -> Option<String> {
        let poster = self.poster.as_ref()?;
        let src = poster.large.as_deref().or(poster.medium.as_deref()).or(poster.small.as_deref())?.trim();
        if src.starts_with("https://") {
            Some(src.to_owned())
        } else if src.starts_with('/') && !src.starts_with("//") {
            Some(format!("{BASE}{src}"))
        } else {
            None
        }
    }

    fn manga(&self) -> Manga {
        let joined = |names: Vec<String>| (!names.is_empty()).then(|| names.join(", "));
        Manga {
            url: self.url(),
            title: self.title.clone(),
            artist: joined(titles(&self.artists, &self.artist_old)),
            author: joined(titles(&self.authors, &self.author_old)),
            description: Some(self.description()),
            genre: Some(self.genres()),
            status: match self.status.as_str() {
                "releasing" => status::ONGOING,
                "on_hiatus" => status::ON_HIATUS,
                "finished" => status::COMPLETED,
                "discontinued" => status::CANCELLED,
                _ => status::UNKNOWN,
            },
            thumbnail_url: self.cover(),
        }
    }

    fn description(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.rated_avg != 0.0 {
            let stars = ((self.rated_avg / 2.0) + 0.5).floor().clamp(0.0, 5.0) as usize;
            parts.push(format!("{}{} {}", "★".repeat(stars), "☆".repeat(5 - stars), self.rated_avg));
        }
        if let Some(synopsis) = self.synopsis.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(synopsis.to_owned());
        }
        let mut facts: Vec<String> = Vec::new();
        if let Some(year) = self.year.filter(|y| *y > 0) {
            facts.push(format!("**Year**: {year}"));
        }
        if let Some(language) = self.original_language.as_deref().filter(|l| !l.trim().is_empty()) {
            facts.push(format!("**Language**: {}", language.to_uppercase()));
        }
        if let Some(first) = self.content_rating.chars().next() {
            facts.push(format!("**Content rating**: {}{}", first.to_uppercase(), &self.content_rating[first.len_utf8()..]));
        }
        for (label, count) in [("Rank", self.rank), ("Rated by", self.rated_count), ("Followed by", self.follows_total)] {
            if count > 0 {
                facts.push(if label == "Rank" { format!("**Rank**: #{count}") } else { format!("**{label}**: {count}") });
            }
        }
        if !facts.is_empty() {
            parts.push(facts.join("\n"));
        }
        let trackers = self.tracker_links();
        if !trackers.is_empty() {
            parts.push(format!("**Trackers**:\n{}", trackers.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n")));
        }
        parts.join("\n\n")
    }

    fn tracker_links(&self) -> Vec<String> {
        const SITES: [(&str, &str); 5] = [("al", "AniList"), ("mal", "MyAnimeList"), ("mu", "MangaUpdates"), ("md", "MangaDex"), ("mb", "MangaBaka")];
        let Some(links) = &self.links else { return Vec::new() };
        let known = SITES.iter().filter_map(|(key, name)| links.get(*key)?.as_deref().filter(|u| !u.trim().is_empty()).map(|u| format!("[{name}]({u})")));
        let unknown = links.iter().filter(|(k, _)| !SITES.iter().any(|(s, _)| s == k)).filter_map(|(k, u)| u.as_deref().filter(|u| u.starts_with("http")).map(|u| format!("[{}]({u})", k.to_uppercase())));
        known.chain(unknown).collect()
    }

    fn genres(&self) -> String {
        let kind = match self.kind.as_str() {
            "manhwa" => "Manhwa",
            "manhua" => "Manhua",
            "manga" => "Manga",
            _ => "Other",
        };
        let mut out = vec![kind.to_owned()];
        out.extend(titles(&self.genres, &self.genre_old));
        out.extend(titles(&self.demographics, &self.demographic_old));
        if matches!(self.content_rating.as_str(), "erotica" | "pornographic") {
            out.push("NSFW".to_owned());
        }
        let mut seen: Vec<String> = Vec::new();
        out.retain(|genre| if seen.contains(genre) { false } else { seen.push(genre.clone()); true });
        out.join(", ")
    }
}

/// The JSON inside `<script id="initial-data">`. The script is raw text, so it ends at the first `</script>`.
fn initial_data(html: &str) -> Option<&str> {
    let at = html.find("id=\"initial-data\"")?;
    let open = at + html[at..].find('>')? + 1;
    let close = open + html[open..].find("</script>")?;
    Some(&html[open..close])
}

/// The details of a title page: the `detail` query of its initial data. The address is the detail's own.
pub fn details(html: &str) -> Res<Manga> {
    let data = initial_data(html).ok_or("Comix: the title page carries no data")?;
    let value: Value = serde_json::from_str(data).map_err(|e| format!("Comix: the title page's data could not be read: {e}"))?;
    let detail = value["queries"].as_object().and_then(|queries| queries.iter().find(|(key, _)| key.contains("\"detail\""))).map(|(_, detail)| detail.clone());
    let detail: Detail = serde_json::from_value(detail.ok_or("Comix: the title page's data has no details")?).map_err(|e| format!("Comix: the title's details could not be read: {e}"))?;
    Ok(detail.manga())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLE: &str = include_str!("../fixtures/title.html");

    #[test]
    fn a_title_page_carries_its_details() {
        let manga = details(TITLE).unwrap();
        assert_eq!((manga.url.as_str(), manga.title.as_str(), manga.status), ("/02l05-after-a-meal", "After a Meal", status::ONGOING));
        assert!(manga.thumbnail_url.as_deref().unwrap().starts_with("https://comix.to/images/covers/"), "{:?}", manga.thumbnail_url);
        assert!(manga.genre.as_deref().unwrap().starts_with("Manhwa"), "{:?}", manga.genre);
        let description = manga.description.unwrap();
        assert!(description.contains("**Language**: KO") && description.contains("**Content rating**:"), "{description}");
    }

    #[test]
    fn a_page_without_details_says_so() {
        assert_eq!(details("<html></html>").unwrap_err(), "Comix: the title page carries no data");
        let error = r#"<script type="application/json" id="initial-data">{"page":"error","queries":{}}</script>"#;
        assert_eq!(details(error).unwrap_err(), "Comix: the title page's data has no details");
    }
}
