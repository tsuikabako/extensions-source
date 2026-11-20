//! What Re:ANIME's API and its video host answer, and how each becomes what the app reads. Pure parsing, no requests.
//! The shapes are the ones of the yuzono `en/reanime` extension (`Dto.kt`, Apache-2.0); every field is optional here,
//! so a field the site drops costs that field and not the whole answer.

use kuro_guest::Res;
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video, status};
use serde::Deserialize;
use serde_json::Value;

use crate::{FLIXCLOUD, NAME};

// ================================ Lists ================================

#[derive(Deserialize, Default)]
#[serde(default)]
struct Title {
    english: Option<String>,
    native: Option<String>,
    romaji: Option<String>,
}

impl Title {
    fn all(&self) -> impl Iterator<Item = &str> {
        [&self.english, &self.romaji, &self.native].into_iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty())
    }

    /// English, else romaji, else native.
    fn preferred(&self) -> Option<&str> {
        self.all().next()
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Cover {
    extra_large: Option<String>,
    large: Option<String>,
    medium: Option<String>,
}

impl Cover {
    /// The largest cover; a TMDB poster's largest size is far bigger than a cover needs, so its middle one.
    fn best(&self) -> Option<String> {
        let size = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let (extra_large, large, medium) = (size(&self.extra_large), size(&self.large), size(&self.medium));
        if extra_large.as_deref().is_some_and(|url| url.contains("tmdb.org")) { large.or(medium).or(extra_large) } else { extra_large.or(large).or(medium) }
    }
}

/// A title of a list.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    anime_id: String,
    title: Title,
    cover_image: Cover,
    description: Option<String>,
    status: Option<String>,
    genres: Option<Vec<String>>,
    rating: Option<String>,
}

impl Item {
    /// The site rates its adult titles "Rx - Hentai" and gives them that genre.
    fn is_adult(&self) -> bool {
        self.rating.as_deref().is_some_and(|rating| rating.trim_start().starts_with("Rx")) || self.genres.iter().flatten().any(|genre| genre.eq_ignore_ascii_case("Hentai"))
    }

    fn into_anime(self) -> Option<Anime> {
        let title = self.title.preferred()?.to_owned();
        if !is_slug(&self.anime_id) {
            return None;
        }
        Some(Anime {
            url: self.anime_id,
            title,
            description: self.description.filter(|d| !d.trim().is_empty()),
            thumbnail_url: self.cover_image.best(),
            genre: self.genres.map(|genres| genres.join(", ")).filter(|g| !g.is_empty()),
            status: parse_status(self.status.as_deref()),
            ..Default::default()
        })
    }
}

fn animes(items: Vec<Item>, show_adult: bool) -> Vec<Anime> {
    items.into_iter().filter(|item| show_adult || !item.is_adult()).filter_map(Item::into_anime).collect()
}

fn parse_status(status: Option<&str>) -> i32 {
    match status.unwrap_or_default().to_ascii_uppercase().as_str() {
        "RELEASING" => status::ONGOING,
        "FINISHED" => status::COMPLETED,
        "CANCELLED" => status::CANCELLED,
        _ => status::UNKNOWN,
    }
}

/// A title's stored address is its id on the site, as the Kotlin stores it: lower-case letters, digits and '-'.
pub fn is_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.len() <= 200 && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn read<T: for<'de> Deserialize<'de>>(body: &str, what: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("{NAME}: the {what} could not be read: {e}"))
}

/// A page of `/api/v1/search`: the titles, and whether the site has more after them.
pub fn search(body: &str, show_adult: bool) -> Res<AnimesPage> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Page {
        limit: i64,
        offset: i64,
        results: Vec<Item>,
        total: i64,
    }
    let page: Page = read(body, "list")?;
    Ok(AnimesPage { has_next_page: page.offset + page.limit < page.total, animes: animes(page.results, show_adult) })
}

/// A page of `/api/v1/home/latest-aired` and the cursor of the page after it (`None` on the last page).
pub fn latest(body: &str, show_adult: bool) -> Res<(AnimesPage, Option<String>)> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Page {
        data: Vec<Item>,
        has_more: bool,
        next_cursor: Option<String>,
    }
    let page: Page = read(body, "list")?;
    let cursor = page.next_cursor.filter(|cursor| page.has_more && !cursor.is_empty());
    Ok((AnimesPage { has_next_page: cursor.is_some(), animes: animes(page.data, show_adult) }, cursor))
}

// ================================ Details ================================

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
struct FuzzyDate {
    year: Option<i64>,
    month: Option<i64>,
    day: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Named {
    name: Option<String>,
    is_main: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NextAiring {
    airing_at: Option<String>,
    episode: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Seasons {
    tmdb: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Trailer {
    id: Option<String>,
    site: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Link {
    site: Option<String>,
    url: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// `/api/v1/anime/<slug>`.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Details {
    anilist_id: Option<i64>,
    mal_id: Option<i64>,
    kitsu_id: Option<i64>,
    anidb_id: Option<i64>,
    anime_planet_id: Option<String>,
    animenewsnetwork_id: Option<i64>,
    anisearch_id: Option<i64>,
    simkl_id: Option<i64>,
    themoviedb_id: Option<i64>,
    tvdb_id: Option<i64>,
    imdb_id: Option<String>,
    external_seasons: Option<Seasons>,
    title: Title,
    cover_image: Cover,
    description: Option<String>,
    format: Option<String>,
    status: Option<String>,
    genres: Option<Vec<String>>,
    tags: Option<Vec<Named>>,
    source: Option<String>,
    country_of_origin: Option<String>,
    season: Option<String>,
    season_year: Option<i64>,
    start_date: Option<FuzzyDate>,
    end_date: Option<FuzzyDate>,
    next_airing_episode: Option<NextAiring>,
    /// Minutes as a number here, a text ("24m") in the lists.
    duration: Value,
    average_score: Option<i64>,
    rating: Option<String>,
    studios: Option<Vec<Named>>,
    synonyms: Option<Vec<String>>,
    trailer: Option<Trailer>,
    external_links: Option<Vec<Link>>,
}

/// The ids the video servers of a title are asked by.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct Meta {
    pub slug: String,
    pub anilist: i64,
    pub mal: i64,
    pub tmdb: i64,
    /// The TMDB season; 0 for a film.
    pub season: i64,
}

impl Details {
    pub fn parse(body: &str) -> Res<Self> {
        read(body, "title")
    }

    pub fn meta(&self, slug: &str) -> Meta {
        let film = self.format.as_deref().is_some_and(|format| format.eq_ignore_ascii_case("MOVIE"));
        let season = if film { 0 } else { self.external_seasons.as_ref().and_then(|s| s.tmdb).filter(|s| *s > 0).unwrap_or(1) };
        Meta { slug: slug.to_owned(), anilist: self.anilist_id.unwrap_or(0), mal: self.mal_id.unwrap_or(0), tmdb: self.themoviedb_id.unwrap_or(0), season }
    }

    /// The title as the app shows it; `url` is the stored address the details were asked with.
    pub fn anime(&self, url: &str) -> Anime {
        let mut genres: Vec<&str> = self.genres.iter().flatten().map(|g| g.trim()).filter(|g| !g.is_empty()).collect();
        let mut tags: Vec<&str> = self.tags.iter().flatten().filter_map(|tag| tag.name.as_deref()).map(str::trim).filter(|t| !t.is_empty()).collect();
        tags.sort_unstable();
        for tag in tags {
            if !genres.contains(&tag) {
                genres.push(tag);
            }
        }
        let studios: Vec<&str> = self.studios.iter().flatten().filter(|s| s.is_main == Some(true)).filter_map(|s| s.name.as_deref()).collect();
        Anime {
            url: url.to_owned(),
            title: self.title.preferred().unwrap_or(url).to_owned(),
            description: Some(self.description()).filter(|d| !d.is_empty()),
            thumbnail_url: self.cover_image.best(),
            author: (!studios.is_empty()).then(|| studios.join(", ")),
            artist: None,
            genre: (!genres.is_empty()).then(|| genres.join(", ")),
            status: parse_status(self.status.as_deref()),
        }
    }

    /// The Kotlin's description: the score as stars, the synopsis, then one fact a line and the links to the trackers,
    /// the legal streams and the trailer.
    fn description(&self) -> String {
        let mut out = String::new();
        if let Some(score) = self.average_score.filter(|score| *score > 0) {
            let stars = ((score as f64 / 20.0).round() as usize).clamp(1, 5);
            out.push_str(&format!("{}{} {score}\n\n", "★".repeat(stars), "☆".repeat(5 - stars)));
        }
        let synopsis = self.description.as_deref().map(plain_text).unwrap_or_default();
        if !synopsis.is_empty() {
            out.push_str(&format!("{synopsis}\n\n"));
        }

        let mut facts: Vec<String> = Vec::new();
        let preferred = self.title.preferred();
        let mut names: Vec<&str> = Vec::new();
        for name in self.title.all().chain(self.synonyms.iter().flatten().map(|s| s.trim()).filter(|s| !s.is_empty())) {
            if Some(name) != preferred && !names.contains(&name) {
                names.push(name);
            }
        }
        if !names.is_empty() {
            facts.push(format!("**Alternative Titles**: {}", names.join(" • ")));
        }
        let text = |value: &Option<String>| value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
        if let Some(format) = text(&self.format) {
            facts.push(format!("**Format**: {format}"));
        }
        let source = text(&self.source).map(|source| capitalized(&source.replace('_', " ").to_lowercase()));
        let origin: Vec<String> = [source, text(&self.country_of_origin)].into_iter().flatten().collect();
        if !origin.is_empty() {
            facts.push(format!("**Source**: {}", origin.join(" • ")));
        }
        match (self.start_date.and_then(fuzzy_date), self.end_date.and_then(fuzzy_date)) {
            (Some(start), Some(end)) if start == end => facts.push(format!("**Air Date**: On {start}")),
            (Some(start), Some(end)) => facts.push(format!("**Airing**: From {start} to {end}")),
            (Some(start), None) => facts.push(format!("**Start Date**: {start}")),
            (None, Some(end)) => facts.push(format!("**End Date**: {end}")),
            (None, None) => {}
        }
        if let Some(next) = &self.next_airing_episode {
            let day = next.airing_at.as_deref().and_then(|at| {
                let mut parts = at.split('T').next()?.split('-').map(|part| part.parse::<i64>().ok());
                fuzzy_date(FuzzyDate { year: parts.next()?, month: parts.next()?, day: parts.next()? })
            });
            if let (Some(episode), Some(day)) = (next.episode, day) {
                facts.push(format!("**Next Airing**: Episode {episode} on {day}"));
            }
        }
        if let (Some(season), Some(year)) = (text(&self.season).filter(|s| s != "0"), self.season_year.filter(|y| *y > 0)) {
            facts.push(format!("**Season**: {} {year}", capitalized(&season)));
        }
        if let Some(minutes) = self.duration.as_i64().filter(|m| *m > 0) {
            facts.push(format!("**Duration**: {minutes}m"));
        }
        if let Some(rating) = text(&self.rating) {
            facts.push(format!("**Rating**: {rating}"));
        }
        if !facts.is_empty() {
            out.push_str(&format!("{}\n", facts.join("\n")));
        }

        let number = |id: Option<i64>, name: &str, address: &str| id.filter(|id| *id > 0).map(|id| format!("[{name}]({address}{id})"));
        let word = |id: &Option<String>, name: &str, address: &str| text(id).map(|id| format!("[{name}]({address}{id})"));
        let trackers: Vec<String> = [
            number(self.anilist_id, "AniList", "https://anilist.co/anime/"),
            number(self.mal_id, "MAL", "https://myanimelist.net/anime/"),
            number(self.kitsu_id, "Kitsu", "https://kitsu.io/anime/"),
            number(self.anidb_id, "AniDB", "https://anidb.net/anime/"),
            word(&self.anime_planet_id, "Anime-Planet", "https://www.anime-planet.com/anime/"),
            number(self.animenewsnetwork_id, "ANN", "https://www.animenewsnetwork.com/encyclopedia/anime.php?id="),
            number(self.anisearch_id, "Anisearch", "https://www.anisearch.com/anime/"),
            number(self.simkl_id, "Simkl", "https://simkl.com/anime/"),
            number(self.themoviedb_id, "TMDB", "https://www.themoviedb.org/tv/"),
            number(self.tvdb_id, "TVDB", "https://thetvdb.com/series/"),
            word(&self.imdb_id, "IMDB", "https://www.imdb.com/title/"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !trackers.is_empty() {
            out.push_str(&format!("**Trackers**: {}\n", trackers.join(" • ")));
        }
        let streams: Vec<String> = self
            .external_links
            .iter()
            .flatten()
            .filter(|link| link.kind.as_deref() == Some("STREAMING"))
            .filter_map(|link| Some(format!("[{}]({})", link.site.as_deref()?, link.url.as_deref()?)))
            .collect();
        if !streams.is_empty() {
            out.push_str(&format!("**Streaming**: {}\n", streams.join(" • ")));
        }
        if let Some(id) = self.trailer.as_ref().filter(|t| t.site.as_deref() == Some("youtube")).and_then(|t| text(&t.id)) {
            out.push_str(&format!("**Trailer**: [YouTube](https://www.youtube.com/watch?v={id})\n"));
        }
        out.trim().to_owned()
    }
}

fn capitalized(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters.map(|c| c.to_ascii_lowercase())).collect()).unwrap_or_default()
}

/// A synopsis without its markup: `<br>` is a line break, every other tag is dropped.
fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = &rest[start..];
            break;
        };
        let tag = rest[start + 1..start + end].trim().to_ascii_lowercase();
        if tag == "br" || tag.starts_with("br ") || tag.starts_with("br/") {
            out.push('\n');
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out.trim().to_owned()
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "October 19th, 2014", or "October 2014" for a date without its day. `None` without a year or a month.
fn fuzzy_date(date: FuzzyDate) -> Option<String> {
    let year = date.year.filter(|year| *year > 0)?;
    let month = MONTHS[usize::try_from(date.month.filter(|month| (1..=12).contains(month))? - 1).ok()?];
    Some(match date.day.filter(|day| *day > 0) {
        Some(day) => {
            let ordinal = match (day % 100, day % 10) {
                (11..=13, _) => "th",
                (_, 1) => "st",
                (_, 2) => "nd",
                (_, 3) => "rd",
                _ => "th",
            };
            format!("{month} {day}{ordinal}, {year}")
        }
        None => format!("{month} {year}"),
    })
}

// ================================ Episodes ================================

/// The episodes of `/api/v1/anime/<slug>/episodes`, newest first. An episode's stored address is the Kotlin's:
/// `<slug>/<episode id>`, the id being `ep-<n>`.
pub fn episodes(body: &str, slug: &str) -> Res<Vec<Episode>> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct List {
        data: Vec<Item>,
    }
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "snake_case")]
    struct Item {
        #[serde(rename = "episodeId")]
        episode_id: Option<String>,
        episode_number: f64,
        title: Option<String>,
        aired: Option<String>,
        is_filler: bool,
        is_recap: bool,
        subbed: bool,
        dubbed: bool,
        description: Option<String>,
    }
    let list: List = read(body, "episode list")?;
    let mut episodes: Vec<Episode> = list
        .data
        .into_iter()
        .map(|episode| {
            let number = if episode.episode_number.fract() == 0.0 { format!("{}", episode.episode_number as i64) } else { format!("{}", episode.episode_number) };
            let id = episode.episode_id.filter(|id| is_episode_id(id)).unwrap_or_else(|| format!("ep-{}", episode.episode_number as i64));
            let title = episode.title.as_deref().map(str::trim).unwrap_or_default();
            let mut name = if title.is_empty() || title.to_lowercase().contains("episode") { format!("Episode {number}") } else { format!("Episode {number} - {title}") };
            if episode.is_recap {
                name.push_str(" [Recap]");
            }
            Episode {
                url: format!("{slug}/{id}"),
                name,
                episode_number: episode.episode_number as f32,
                fillermark: episode.is_filler,
                scanlator: match (episode.subbed, episode.dubbed) {
                    (true, true) => Some("Sub & Dub".into()),
                    (true, false) => Some("Sub".into()),
                    (false, true) => Some("Dub".into()),
                    (false, false) => None,
                },
                date_upload: episode.aired.as_deref().and_then(aired_millis).unwrap_or(0),
                summary: episode.description.map(|d| plain_text(&d)).filter(|d| !d.is_empty()),
            }
        })
        .collect();
    if episodes.is_empty() {
        return Err(format!("{NAME} lists no episodes for this title"));
    }
    episodes.reverse();
    Ok(episodes)
}

fn is_episode_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

/// "2014-11-30T00:00:00Z" (UTC, fractions of a second ignored) as milliseconds since 1970.
fn aired_millis(aired: &str) -> Option<i64> {
    let seconds = aired.get(..19)?;
    chrono::NaiveDateTime::parse_from_str(seconds, "%Y-%m-%dT%H:%M:%S").ok().map(|time| time.and_utc().timestamp_millis())
}

/// The two parts of an episode's stored address: the title's slug and the number the video servers are asked by.
pub fn episode_parts(url: &str) -> Res<(&str, &str)> {
    let parts = url.split_once('/').map(|(slug, id)| (slug, id.strip_prefix("ep-").unwrap_or(id)));
    parts.filter(|(slug, number)| is_slug(slug) && is_episode_id(number)).ok_or_else(|| format!("not a {NAME} episode: {url}"))
}

// ================================ Servers ================================

/// The file of a player link of the video host (`https://flixcloud.cc/e/<file>?v=<n>`): its id, and the link's query.
fn player_file(link: &str) -> Option<(String, String)> {
    let address = url::Url::parse(link).ok().filter(|u| u.scheme() == "https" && u.host_str().is_some_and(|host| FLIXCLOUD.strip_prefix("https://") == Some(host)))?;
    let id = address.path().strip_prefix("/e/").filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))?;
    let version = address.query_pairs().find(|(name, _)| name == "v").map(|(_, v)| v.into_owned()).filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    Some((id.to_owned(), version.map(|v| format!("?v={v}")).unwrap_or_default()))
}

/// The servers of `/api/flix/...` as hosters: one per file. The site lists a file once for each of its players and
/// audio types ("HD-1" sub, "HD-1" dub, "HD-2" ...); they all lead to the one file, which carries every audio track.
/// `hosterUrl` is where the file's download data is asked.
pub fn hosters(body: &str) -> Res<Vec<Hoster>> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Answer {
        success: bool,
        servers: Option<Vec<Server>>,
    }
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct Server {
        data_link: Option<String>,
        data_type: Option<String>,
    }
    let answer: Answer = read(body, "server list")?;
    // (file, query of its first link, audio types in the site's order)
    let mut files: Vec<(String, String, Vec<String>)> = Vec::new();
    for server in answer.servers.into_iter().flatten().filter(|_| answer.success) {
        let Some((file, query)) = server.data_link.as_deref().and_then(player_file) else { continue };
        let audio = match server.data_type.as_deref().map(str::trim).unwrap_or_default() {
            "sub" => "Sub".to_owned(),
            "dub" => "Dub".to_owned(),
            other => capitalized(other),
        };
        match files.iter_mut().find(|(seen, _, _)| *seen == file) {
            Some((_, _, audios)) if !audio.is_empty() && !audios.contains(&audio) => audios.push(audio),
            Some(_) => {}
            None => files.push((file, query, if audio.is_empty() { Vec::new() } else { vec![audio] })),
        }
    }
    Ok(files
        .into_iter()
        .map(|(file, query, audios)| Hoster {
            hoster_url: format!("{FLIXCLOUD}/d/{file}/__data.json{query}"),
            hoster_name: if audios.is_empty() { "FlixCloud".to_owned() } else { format!("FlixCloud ({})", audios.join(" & ")) },
            ..Default::default()
        })
        .collect())
}

/// Whether `url` is a download-data address this source made (the app hands a hoster back as it got it).
pub fn is_download_data(url: &str) -> bool {
    url.strip_prefix(FLIXCLOUD).and_then(|path| path.strip_prefix("/d/")).and_then(|rest| rest.split_once("/__data.json")).is_some_and(|(file, query)| {
        !file.is_empty() && file.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) && (query.is_empty() || query.strip_prefix("?v=").is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())))
    })
}

/// The file of a download-data answer of the video host, as the one video of its hoster: the original MKV file
/// through the host's download address, which redirects to a signed link.
///
/// The answer is a SvelteKit data file: one flat list in which an object's values are positions in that list.
pub fn download(body: &str) -> Res<Video> {
    let shape = || format!("{NAME}: the video host's answer has changed");
    let answer: Value = read(body, "video host's answer")?;
    let list = answer["nodes"].as_array().into_iter().flatten().find_map(|node| node["data"].as_array().filter(|data| data.first().is_some_and(|root| root.get("download").is_some()))).ok_or_else(shape)?;
    let at = |object: &Value, key: &str| object.get(key).and_then(Value::as_u64).and_then(|index| list.get(usize::try_from(index).ok()?));
    let text = |object: &Value, key: &str| at(object, key).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
    let root = &list[0];
    let (video, download) = (at(root, "video").ok_or_else(shape)?, at(root, "download").ok_or_else(shape)?);
    let (file, base, token) = (text(video, "fileId").ok_or_else(shape)?, text(download, "base").ok_or_else(shape)?, text(download, "token").ok_or_else(shape)?);
    let safe = |value: &str, extra: &[u8]| value.bytes().all(|b| b.is_ascii_alphanumeric() || extra.contains(&b));
    if !base.starts_with("https://") || !safe(&file, b"-") || !safe(&token, b"-_.") {
        return Err(shape());
    }
    let resolution = text(video, "resolution").filter(|r| r != "null");
    let height: Option<i32> = resolution.as_deref().and_then(|r| r.trim_end_matches(['p', 'P']).parse().ok()).filter(|h| *h > 0);
    Ok(Video {
        video_url: format!("{}/download/{file}?token={token}", base.trim_end_matches('/')),
        video_title: resolution.map_or_else(|| "MKV".to_owned(), |r| format!("{r} MKV")),
        resolution: height,
        headers: vec![("Referer".into(), format!("{FLIXCLOUD}/"))],
        preferred: true,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_episode_addresses_are_checked() {
        assert!(is_slug("mushi-shi-the-next-passage-2-qazz9g"));
        assert!(!is_slug("") && !is_slug("a/b") && !is_slug("A") && !is_slug("a?x=1") && !is_slug(".."));
        assert_eq!(episode_parts("a-b/ep-7"), Ok(("a-b", "7")));
        assert_eq!(episode_parts("a-b/ep-7.5"), Ok(("a-b", "7.5")));
        assert_eq!(episode_parts("a-b/12"), Ok(("a-b", "12")));
        assert!(episode_parts("a-b").is_err() && episode_parts("a-b/ep-1/x").is_err() && episode_parts("../ep-1").is_err() && episode_parts("a-b/ep-1?x").is_err());
    }

    #[test]
    fn dates_read_as_the_kotlin_writes_them() {
        let date = |year, month, day| fuzzy_date(FuzzyDate { year, month, day });
        assert_eq!(date(Some(2014), Some(10), Some(19)).as_deref(), Some("October 19th, 2014"));
        assert_eq!(date(Some(2026), Some(1), Some(1)).as_deref(), Some("January 1st, 2026"));
        assert_eq!(date(Some(2026), Some(2), Some(22)).as_deref(), Some("February 22nd, 2026"));
        assert_eq!(date(Some(2026), Some(3), Some(3)).as_deref(), Some("March 3rd, 2026"));
        assert_eq!(date(Some(2026), Some(4), Some(12)).as_deref(), Some("April 12th, 2026"));
        assert_eq!(date(Some(2026), Some(12), None).as_deref(), Some("December 2026"));
        assert_eq!(date(Some(2026), Some(13), Some(1)), None);
        assert_eq!(date(None, Some(1), Some(1)), None);
        assert_eq!(aired_millis("2014-11-30T00:00:00Z"), Some(1_417_305_600_000));
        assert_eq!(aired_millis("2026-10-02T15:15:29.235Z"), Some(1_790_954_129_000));
        assert_eq!(aired_millis("soon"), None);
    }

    #[test]
    fn a_synopsis_loses_its_markup() {
        assert_eq!(plain_text("One.\n<br><br>\nTwo <i>three</i>.<BR/>Four"), "One.\n\n\n\nTwo three.\nFour");
        assert_eq!(plain_text("a < b"), "a < b");
        assert_eq!(capitalized("LIGHT novel"), "Light novel");
    }

    #[test]
    fn adult_titles_are_left_out_unless_asked_for() {
        let body = r#"{"limit":3,"offset":0,"total":3,"results":[
            {"anime_id":"a-1","title":{"english":"A"},"rating":"PG-13 - Teens 13 or older","genres":["Action"]},
            {"anime_id":"b-2","title":{"romaji":"B"},"rating":"Rx - Hentai","genres":["Romance"]},
            {"anime_id":"c-3","title":{"native":"C"},"rating":"","genres":["Comedy","Hentai"]},
            {"anime_id":"d-4","title":{"english":" "}}]}"#;
        let titles = |show| search(body, show).unwrap().animes.into_iter().map(|a| a.title).collect::<Vec<_>>();
        assert_eq!(titles(false), ["A"]);
        assert_eq!(titles(true), ["A", "B", "C"]);
        assert!(!search(body, true).unwrap().has_next_page);
    }

    #[test]
    fn servers_of_one_file_are_one_hoster() {
        let body = r#"{"success":true,"servers":[
            {"serverName":"HD-1","dataLink":"https://flixcloud.cc/e/abc123?v=1","dataType":"sub"},
            {"serverName":"HD-1","dataLink":"https://flixcloud.cc/e/abc123?v=1","dataType":"dub"},
            {"serverName":"HD-2","dataLink":"https://flixcloud.cc/e/abc123?v=2","dataType":"sub"},
            {"serverName":"HD-1","dataLink":"https://flixcloud.cc/e/zzz9","dataType":"dub"},
            {"serverName":"X","dataLink":"https://evil.example/e/abc123?v=1","dataType":"sub"},
            {"serverName":"Y","dataLink":"https://flixcloud.cc/e/../d?v=1","dataType":"sub"},
            {"serverName":"Z"}]}"#;
        let hosters = hosters(body).unwrap();
        let seen: Vec<(&str, &str)> = hosters.iter().map(|h| (h.hoster_name.as_str(), h.hoster_url.as_str())).collect();
        assert_eq!(seen, [("FlixCloud (Sub & Dub)", "https://flixcloud.cc/d/abc123/__data.json?v=1"), ("FlixCloud (Dub)", "https://flixcloud.cc/d/zzz9/__data.json")]);
        assert!(hosters.iter().all(|h| is_download_data(&h.hoster_url)));
        assert!(!is_download_data("https://flixcloud.cc/d/abc/__data.json?v=1&x=2") && !is_download_data("https://evil.example/d/abc/__data.json") && !is_download_data("https://flixcloud.cc/d//__data.json"));
        assert_eq!(super::hosters(r#"{"success":false,"servers":[{"dataLink":"https://flixcloud.cc/e/abc123"}]}"#).unwrap().len(), 0);
    }

    #[test]
    fn a_changed_download_answer_is_said_so() {
        let changed = "Re:ANIME: the video host's answer has changed";
        assert_eq!(download(r#"{"nodes":[null,{"data":[{"video":1},{"fileId":2},"x"]}]}"#).unwrap_err(), changed);
        let odd_base = r#"{"nodes":[{"data":[{"video":1,"download":3},{"fileId":2},"f1",{"base":4,"token":5},"http://plain.example","t"]}]}"#;
        assert_eq!(download(odd_base).unwrap_err(), changed);
        let odd_file = r#"{"nodes":[{"data":[{"video":1,"download":3},{"fileId":2},"f1?x=1",{"base":4,"token":5},"https://fetch1.flixcloud.cc","t"]}]}"#;
        assert_eq!(download(odd_file).unwrap_err(), changed);
        let plain = r#"{"nodes":[{"data":[{"video":1,"download":3},{"fileId":2,"resolution":6},"f1",{"base":4,"token":5},"https://fetch1.flixcloud.cc/","a.b-c_d",null]}]}"#;
        let video = download(plain).unwrap();
        assert_eq!((video.video_url.as_str(), video.video_title.as_str(), video.resolution), ("https://fetch1.flixcloud.cc/download/f1?token=a.b-c_d", "MKV", None));
    }
}
