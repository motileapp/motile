//! What programs look up outside, through your server: photos of what a query names, from Bing's
//! image search, else Wikipedia and Wikimedia Commons, kept in the media store, and the streets a
//! car takes between stops, from OSRM's public server. Photos are kept in the store and routes for
//! a while; failures are not.

use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, bail};
use lru::LruCache;
use motile_protocol::search::fold_char;
use motile_protocol::wire::Media;
use serde_json::Value;

use tokio::sync::Semaphore;

use crate::media::{MediaStore, percent_decode};
use crate::store::Store;

const USER_AGENT: &str = concat!("Motile/", env!("CARGO_PKG_VERSION"), " (https://motile.app)");
const SEARCH_TIMEOUT: Duration = Duration::from_secs(8);
const ROUTE_TIMEOUT: Duration = Duration::from_secs(10);
const PHOTO_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_PHOTO: usize = 2 * 1024 * 1024;
const LOOKUPS_AT_ONCE: usize = 4;
const KEPT_ROUTES: usize = 200;
const MAX_STOPS: usize = 25;
const THUMBNAIL_WIDTH: &str = "960";
const WEB_RESULTS: usize = 4;
/// Bing's thumbnail as wide as Wikimedia's, in its own proportions.
const WEB_THUMBNAIL_SIZE: &str = "w=960&c=7&rs=1";
/// Photos whose names say they are none.
const NOT_PHOTOS: [&str; 6] = [".svg", "icon", "logo", "map", "flag", "seal"];
const NOT_SEARCHED_PHOTOS: [&str; 4] = ["coat_of_arms", "signature", "diagram", "screenshot"];
/// What a web result shows when its title or address says so: no photo of the thing, unless asked.
const NOT_WEB_PHOTOS: [&str; 5] = ["diagram", "wiring", "schematic", "logo", "clipart"];
const LITTLE_WORDS: [&str; 16] =
    ["a", "an", "and", "the", "of", "to", "in", "on", "for", "with", "by", "at", "from", "or", "vs", "s"];

pub struct Lookups {
    http: reqwest::Client,
    bing: String,
    wikipedia: String,
    commons: String,
    osrm: String,
    /// Wikipedia slows down a burst of lookups, so only a few run at once.
    at_once: Semaphore,
    routes: Mutex<LruCache<String, Vec<[f64; 2]>>>,
}

impl Default for Lookups {
    fn default() -> Self {
        Self::new(
            "https://www.bing.com",
            "https://en.wikipedia.org",
            "https://commons.wikimedia.org",
            "https://router.project-osrm.org",
        )
    }
}

impl Lookups {
    pub fn new(bing: &str, wikipedia: &str, commons: &str, osrm: &str) -> Self {
        motile_protocol::tls::install();
        let http = reqwest::Client::builder().user_agent(USER_AGENT).build().unwrap_or_default();
        let kept = |count| NonZeroUsize::new(count).unwrap_or(NonZeroUsize::MIN);
        Self {
            http,
            bing: bing.to_string(),
            wikipedia: wikipedia.to_string(),
            commons: commons.to_string(),
            osrm: osrm.to_string(),
            at_once: Semaphore::new(LOOKUPS_AT_ONCE),
            routes: Mutex::new(LruCache::new(kept(KEPT_ROUTES))),
        }
    }

    /// The photo of what `query` names: the web's, else Wikipedia's or Commons'. Only one is shown,
    /// so only the best that downloads is fetched.
    pub async fn images(&self, media: &MediaStore, store: &Store, query: &str) -> anyhow::Result<Vec<Media>> {
        let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
        if !(2..=200).contains(&query.chars().count()) {
            bail!("A photo is looked up by 2 to 200 characters.");
        }
        let kept =
            || store.ui_photos(&query).ok().flatten().filter(|kept| kept.iter().all(|image| media.has(&image.id)));
        if let Some(kept) = kept() {
            return Ok(kept);
        }
        let _turn = self.at_once.acquire().await?;
        if let Some(kept) = kept() {
            return Ok(kept);
        }
        let web = self.web_photos(&query).await;
        if let Some(photo) = self.first_photo(media, &web).await {
            store.keep_ui_photos(&query, std::slice::from_ref(&photo))?;
            return Ok(vec![photo]);
        }
        let wikimedia = self.wikimedia_photos(&query).await;
        if let Some(photo) = self.first_photo(media, &wikimedia).await {
            store.keep_ui_photos(&query, std::slice::from_ref(&photo))?;
            return Ok(vec![photo]);
        }
        let none_found = |found: &anyhow::Result<Vec<String>>| found.as_ref().is_ok_and(Vec::is_empty);
        if none_found(&web) && none_found(&wikimedia) {
            store.keep_ui_photos(&query, &[])?;
        } else if web.is_err() && wikimedia.is_err() {
            bail!("Neither Bing nor Wikipedia could be reached.");
        }
        Ok(Vec::new())
    }

    /// The first of the photos that downloads, kept in the media store.
    async fn first_photo(&self, media: &MediaStore, urls: &anyhow::Result<Vec<String>>) -> Option<Media> {
        for url in urls.as_ref().ok()? {
            if let Some(photo) = download(&self.http, media.clone(), url).await {
                return Some(photo);
            }
        }
        None
    }

    /// Bing's photos of the query, from the pages whose titles name it, each as its page has it
    /// and then as Bing's thumbnail.
    async fn web_photos(&self, query: &str) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/images/async", self.bing);
        let asked = [("q", query), ("first", "0"), ("count", "20"), ("mmasync", "1")];
        let answer = self.http.get(url).query(&asked).timeout(SEARCH_TIMEOUT).send().await?;
        if !answer.status().is_success() {
            bail!("Bing answered {}.", answer.status());
        }
        Ok(web_photos(&answer.text().await?, query))
    }

    /// The photos of the Wikipedia article with that title, then those of the articles and the
    /// Commons files whose titles name the query, at most four.
    async fn wikimedia_photos(&self, query: &str) -> anyhow::Result<Vec<String>> {
        let (titled, searched) = tokio::join!(self.article_photos(query), self.searched_photos(query));
        let mut photos = titled?;
        for photo in searched? {
            if !photos.contains(&photo) {
                photos.push(photo);
            }
        }
        photos.truncate(4);
        Ok(photos)
    }

    /// The lead photo of the Wikipedia article with that title, then up to two more of its photos;
    /// none when the title leads to another article.
    async fn article_photos(&self, title: &str) -> anyhow::Result<Vec<String>> {
        let page = title.replace(' ', "_");
        let address = |kind: &str| -> anyhow::Result<reqwest::Url> {
            let mut url = reqwest::Url::parse(&self.wikipedia)?;
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Wikipedia's address is wrong."))?
                .extend(["api", "rest_v1", "page", kind, &page]);
            Ok(url)
        };
        let (summary, media) = tokio::join!(self.article(address("summary")?), self.article(address("media-list")?));
        let summary = summary?;
        if summary.is_null() || folded(summary["title"].as_str().unwrap_or_default()) != folded(title) {
            return Ok(Vec::new());
        }
        Ok(article_photos(&summary, &media?))
    }

    /// What the article's address answers; nothing for an article that isn't there.
    async fn article(&self, url: reqwest::Url) -> anyhow::Result<Value> {
        let answer = self.http.get(url).timeout(SEARCH_TIMEOUT).send().await?;
        if answer.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }
        if !answer.status().is_success() {
            bail!("Wikipedia answered {}.", answer.status());
        }
        Ok(answer.json().await?)
    }

    /// The lead photos of the articles and the Commons photos whose titles name the query, those
    /// that name more of it first.
    async fn searched_photos(&self, query: &str) -> anyhow::Result<Vec<String>> {
        let commons_search = format!("{query} filetype:bitmap");
        let articles = [
            ("generator", "search"),
            ("gsrsearch", query),
            ("gsrlimit", "10"),
            ("prop", "pageimages"),
            ("piprop", "thumbnail"),
            ("pithumbsize", THUMBNAIL_WIDTH),
        ];
        let files = [
            ("generator", "search"),
            ("gsrsearch", &commons_search),
            ("gsrnamespace", "6"),
            ("gsrlimit", "10"),
            ("prop", "imageinfo"),
            ("iiprop", "url|mime"),
            ("iiurlwidth", THUMBNAIL_WIDTH),
        ];
        let (articles, files) =
            tokio::join!(self.search(&self.wikipedia, &articles), self.search(&self.commons, &files));
        let (articles, files) = (articles?, files?);
        let wanted = Wanted::new(query);
        let mut found: Vec<(f64, &str)> = Vec::new();
        for page in &articles {
            let (Some(score), Some(url)) = (wanted.score(page["title"].as_str()), page["thumbnail"]["source"].as_str())
            else {
                continue;
            };
            found.push((score, url));
        }
        for file in &files {
            let info = &file["imageinfo"][0];
            let name = file["title"].as_str().map(|title| title.trim_start_matches("File:"));
            let photo = info["mime"].as_str().is_some_and(|mime| mime.starts_with("image/"));
            let (Some(score), Some(url), true) =
                (wanted.score(name), info["thumburl"].as_str().or(info["url"].as_str()), photo)
            else {
                continue;
            };
            found.push((score, url));
        }
        found.sort_by(|(one, _), (other, _)| other.total_cmp(one));
        let mut photos = Vec::new();
        for (_, url) in found {
            add_searched(&mut photos, Some(url));
        }
        Ok(photos)
    }

    /// The pages a MediaWiki query answers with, in the order it ranks them.
    async fn search(&self, site: &str, parameters: &[(&str, &str)]) -> anyhow::Result<Vec<Value>> {
        let url = format!("{site}/w/api.php");
        let fixed = [("action", "query"), ("format", "json"), ("formatversion", "2")];
        let asked = self.http.get(url).query(&fixed).query(parameters).timeout(SEARCH_TIMEOUT);
        let answer = asked.send().await?;
        if !answer.status().is_success() {
            bail!("{site} answered {}.", answer.status());
        }
        let found: Value = answer.json().await?;
        // A rate limit comes back as a success with an error in it.
        if !found["error"].is_null() {
            bail!("{site} refused the search.");
        }
        let mut pages = found["query"]["pages"].as_array().cloned().unwrap_or_default();
        pages.sort_by_key(|page| page["index"].as_i64().unwrap_or(0));
        Ok(pages)
    }

    /// The streets between the stops, each `[lat, lng]`, as OSRM drives them.
    pub async fn route(&self, stops: &[[f64; 2]]) -> anyhow::Result<Vec<[f64; 2]>> {
        let valid = |[lat, lng]: &[f64; 2]| lat.abs() <= 90.0 && lng.abs() <= 180.0;
        if !(2..=MAX_STOPS).contains(&stops.len()) || !stops.iter().all(valid) {
            bail!("A route goes through 2 to {MAX_STOPS} stops, each a latitude and a longitude.");
        }
        let key = stops.iter().map(|[lat, lng]| format!("{lng},{lat}")).collect::<Vec<_>>().join(";");
        if let Some(kept) = self.routes.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key) {
            return Ok(kept.clone());
        }
        let url = format!("{}/route/v1/driving/{key}", self.osrm);
        let asked = [("overview", "full"), ("geometries", "geojson"), ("steps", "false")];
        let answer = self.http.get(url).query(&asked).timeout(ROUTE_TIMEOUT).send().await;
        let found: Value = match answer {
            Ok(answer) if answer.status().is_success() => answer.json().await.unwrap_or_default(),
            _ => bail!("Street routing is unavailable."),
        };
        let points = route_points(&found).context("There is no street route through these stops.")?;
        self.routes.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).put(key, points.clone());
        Ok(points)
    }
}

/// The photo at `url`, kept in the media store; `None` when it couldn't be.
async fn download(http: &reqwest::Client, media: MediaStore, url: &str) -> Option<Media> {
    let url = url.to_string();
    let answer = http.get(&url).timeout(PHOTO_TIMEOUT).send().await.ok()?;
    if !answer.status().is_success() || answer.content_length().is_some_and(|size| size > MAX_PHOTO as u64) {
        return None;
    }
    let bytes = answer.bytes().await.ok().filter(|bytes| bytes.len() <= MAX_PHOTO)?;
    let kept = tokio::task::spawn_blocking(move || media.keep_photo(&bytes, &url)).await.ok()?;
    kept.inspect_err(|error| tracing::warn!("couldn't keep a photo: {error:#}")).ok()?
}

/// The article's lead photo, then its other photos up to three, none of them a map, a logo, a flag
/// or a drawing.
fn article_photos(summary: &Value, media: &Value) -> Vec<String> {
    let mut photos: Vec<String> = Vec::new();
    let lead = summary["thumbnail"]["source"].as_str().filter(|lead| !names_no_photo(lead, &[]));
    photos.extend(lead.map(str::to_string));
    for item in media["items"].as_array().into_iter().flatten() {
        let Some(src) = item["srcset"][0]["src"].as_str().filter(|_| item["type"] == "image") else { continue };
        if names_no_photo(src, &[]) {
            continue;
        }
        let url = match src.strip_prefix("//") {
            Some(rest) => format!("https://{rest}"),
            None => src.to_string(),
        };
        if !photos.iter().any(|photo| file_key(photo) == file_key(&url)) {
            photos.push(url);
        }
        if photos.len() >= 3 {
            break;
        }
    }
    photos
}

/// The photos of Bing's answer whose pages' titles name the query, in its order.
fn web_photos(page: &str, query: &str) -> Vec<String> {
    let wanted = Wanted::new(query);
    let mut photos = Vec::new();
    let results = page.split(" m=\"").skip(1).filter_map(|rest| rest.split('"').next());
    let results = results.filter_map(|result| serde_json::from_str::<Value>(&unescape(result)).ok());
    let asked = query.to_lowercase();
    let drawn = |result: &Value| {
        let named =
            format!("{} {}", result["t"].as_str().unwrap_or_default(), result["murl"].as_str().unwrap_or_default());
        let named = named.to_lowercase();
        NOT_WEB_PHOTOS.iter().any(|word| named.contains(word) && !asked.contains(word))
    };
    let shown = results.filter(|result| wanted.score(result["t"].as_str()).is_some() && !drawn(result));
    for result in shown.take(WEB_RESULTS) {
        let web = |url: &&str| url.starts_with("https://") || url.starts_with("http://");
        photos.extend(result["murl"].as_str().filter(web).map(str::to_string));
        let thumbnail = result["turl"].as_str().filter(web);
        let joined = |url: &str| if url.contains('?') { '&' } else { '?' };
        photos.extend(thumbnail.map(|url| format!("{url}{}{WEB_THUMBNAIL_SIZE}", joined(url))));
    }
    photos
}

/// An HTML attribute's text as written.
fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The file a photo's address names, whatever size it was scaled to.
fn file_key(url: &str) -> String {
    let file = url.split('?').next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
    let file = percent_decode(file);
    match file.split_once("px-") {
        Some((width, rest)) if !width.is_empty() && width.chars().all(|digit| digit.is_ascii_digit()) => {
            rest.to_string()
        }
        _ => file,
    }
}

fn names_no_photo(url: &str, more: &[&str]) -> bool {
    let url = url.to_lowercase();
    NOT_PHOTOS.iter().chain(more).any(|word| url.contains(word))
}

fn add_searched(photos: &mut Vec<String>, url: Option<&str>) {
    let Some(url) = url.filter(|url| url.starts_with("https://")) else { return };
    if !names_no_photo(url, &NOT_SEARCHED_PHOTOS) && !photos.iter().any(|photo| photo == url) {
        photos.push(url.to_string());
    }
}

/// What a photo's title has to name of a query: every word with a digit, a model's or a year's,
/// and more than half of the others.
struct Wanted {
    words: Vec<String>,
}

impl Wanted {
    fn new(query: &str) -> Self {
        Self { words: words(query) }
    }

    /// How much of the query the title names, or `None` when it doesn't name the thing.
    fn score(&self, title: Option<&str>) -> Option<f64> {
        let named = words(title?);
        let has = |word: &&&String| named.iter().any(|other| same_word(word, other));
        let (numbered, plain): (Vec<&String>, Vec<&String>) = self.words.iter().partition(|word| has_digit(word));
        let found = plain.iter().filter(has).count();
        if self.words.is_empty()
            || !numbered.iter().all(|word| has(&word))
            || found * 2 <= plain.len() && !plain.is_empty()
        {
            return None;
        }
        Some((numbered.len() + found) as f64 / self.words.len() as f64)
    }
}

/// The words of a text that say what it shows, folded, without the little ones.
fn words(text: &str) -> Vec<String> {
    let folded = folded(text);
    let words = folded.split(|letter: char| !letter.is_alphanumeric()).filter(|word| !word.is_empty());
    words.filter(|word| !LITTLE_WORDS.contains(word)).map(str::to_string).collect()
}

fn folded(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn has_digit(word: &str) -> bool {
    word.chars().any(|letter| letter.is_ascii_digit())
}

/// The same word, or one a few letters longer: "towers" for "tower", "controller" for "control",
/// but never another model, "m40" for "m4".
fn same_word(one: &str, other: &str) -> bool {
    let (short, long) = if one.len() <= other.len() { (one, other) } else { (other, one) };
    if short == long {
        return true;
    }
    let Some(rest) = long.strip_prefix(short) else { return false };
    !has_digit(long) && (rest == "s" || short.chars().count() >= 4 && rest.chars().count() <= 3)
}

/// OSRM's route as `[lat, lng]` points; `None` without one of at least two.
fn route_points(found: &Value) -> Option<Vec<[f64; 2]>> {
    if found["code"] != "Ok" {
        return None;
    }
    let coordinates = found["routes"][0]["geometry"]["coordinates"].as_array()?;
    let points: Vec<[f64; 2]> =
        coordinates.iter().filter_map(|point| Some([point[1].as_f64()?, point[0].as_f64()?])).collect();
    (points.len() >= 2).then_some(points)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::{StatusCode, Uri, header};
    use axum::response::{IntoResponse, Response};
    use serde_json::json;

    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    /// A Bing result as its answer writes it, in an attribute.
    fn web_result(title: &str, full: &str, thumbnail: &str) -> String {
        let result = json!({ "t": title, "murl": full, "turl": thumbnail }).to_string();
        format!(r#"<a class="iusc" m="{}" href="/images">"#, result.replace('&', "&amp;").replace('"', "&quot;"))
    }

    /// Bing, Wikipedia, Commons and OSRM on one local address, and what each was asked.
    async fn stand_in() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (here, seen) = (base.clone(), asked.clone());
        let answer = move |uri: Uri| {
            let (base, seen) = (here.clone(), seen.clone());
            async move {
                let path = uri.path().to_string();
                let query = percent_decode(uri.query().unwrap_or_default()).replace('+', " ");
                seen.lock().unwrap().push(format!("{path}?{query}"));
                let json =
                    |value: Value| ([(header::CONTENT_TYPE, "application/json")], value.to_string()).into_response();
                let found: Response = match path.as_str() {
                    "/images/async" if query.contains("Broken") => StatusCode::BAD_GATEWAY.into_response(),
                    "/images/async" => [
                        web_result("Embarcadero at sunset", &format!("{base}/photos/Inside.png"), ""),
                        web_result(
                            "The Ferry Building &amp; its clock tower",
                            &format!("{base}/photos/gone.png"),
                            &format!("{base}/photos/320px-Ferry.png"),
                        ),
                    ]
                    .concat()
                    .into_response(),
                    "/api/rest_v1/page/summary/Ferry_Building" => json(json!({
                        "title": "Ferry Building",
                        "thumbnail": { "source": format!("{base}/photos/320px-Ferry.png") },
                    })),
                    "/api/rest_v1/page/media-list/Ferry_Building" => json(json!({ "items": [
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/640px-Ferry.png") }] },
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/Map_of_the_pier.png") }] },
                        { "type": "video", "srcset": [{ "src": format!("{base}/photos/tour.png") }] },
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/Inside.png") }] },
                    ] })),
                    "/api/rest_v1/page/summary/Ferry_terminal" => json(json!({
                        "title": "Ferry",
                        "thumbnail": { "source": format!("{base}/photos/320px-Ferry.png") },
                    })),
                    "/w/api.php" if query.contains("Broken") => StatusCode::BAD_GATEWAY.into_response(),
                    "/w/api.php" if query.contains("gsrnamespace=6") => json(json!({ "query": { "pages": [
                        { "index": 1, "title": "File:2022 Ram Heavy Duty, front left.jpg", "imageinfo": [{ "mime": "image/jpeg", "thumburl": "https://127.0.0.1:1/truck.jpg" }] },
                        { "index": 2, "title": "File:Ferry Building at dusk.png", "imageinfo": [{ "mime": "image/png", "thumburl": "https://127.0.0.1:1/commons.png" }] },
                        { "index": 3, "title": "File:Ferry Building plan.svg", "imageinfo": [{ "mime": "image/svg+xml", "url": "https://127.0.0.1:1/plan.svg" }] },
                    ] } })),
                    "/w/api.php" => json(json!({ "query": { "pages": [
                        { "index": 2, "title": "Embarcadero", "thumbnail": { "source": "https://127.0.0.1:1/other.png" } },
                        { "index": 1, "title": "Ferry Building", "thumbnail": { "source": "https://127.0.0.1:1/logo.png" } },
                    ] } })),
                    "/photos/320px-Ferry.png" => png(320, 200).into_response(),
                    "/photos/Inside.png" => png(640, 480).into_response(),
                    route if route.starts_with("/route/v1/driving/-122.39,37.79;") => json(json!({
                        "code": "Ok", "routes": [{ "geometry": { "coordinates": [[-122.39, 37.79], [-122.4, 37.8]] } }],
                    })),
                    route if route.starts_with("/route/v1/driving/") => json(json!({ "code": "NoRoute" })),
                    _ => StatusCode::NOT_FOUND.into_response(),
                };
                found
            }
        };
        tokio::spawn(async move { axum::serve(listener, axum::Router::new().fallback(answer)).await });
        (base, asked)
    }

    fn lookups(base: &str) -> Lookups {
        Lookups::new(base, base, base, base)
    }

    fn photos_asked(asked: &Mutex<Vec<String>>) -> Vec<String> {
        let asked = asked.lock().unwrap();
        let photos = asked.iter().filter_map(|asked| asked.strip_prefix("/photos/"));
        photos.map(|photo| photo.split('?').next().unwrap_or_default().to_string()).collect()
    }

    #[test]
    fn an_articles_photos_are_its_lead_then_two_more_that_are_photos() {
        let summary = json!({ "thumbnail": { "source": "https://upload.wikimedia.org/a/320px-Pier.jpg" } });
        let media = json!({ "items": [
            { "type": "image", "srcset": [{ "src": "//upload.wikimedia.org/a/640px-Pier.jpg" }] },
            { "type": "image", "srcset": [{ "src": "//upload.wikimedia.org/a/Flag_of_X.svg.png" }] },
            { "type": "image", "srcset": [{ "src": "//upload.wikimedia.org/a/Clock%20tower.jpg" }] },
            { "type": "image", "srcset": [{ "src": "//upload.wikimedia.org/a/Hall.jpg" }] },
            { "type": "image", "srcset": [{ "src": "//upload.wikimedia.org/a/Roof.jpg" }] },
        ] });

        assert_eq!(
            article_photos(&summary, &media),
            [
                "https://upload.wikimedia.org/a/320px-Pier.jpg",
                "https://upload.wikimedia.org/a/Clock%20tower.jpg",
                "https://upload.wikimedia.org/a/Hall.jpg",
            ]
        );
        let logo = json!({ "thumbnail": { "source": "https://upload.wikimedia.org/a/330px-Pi_Logo.svg.png" } });
        assert!(article_photos(&logo, &Value::Null).is_empty());
        assert!(article_photos(&Value::Null, &Value::Null).is_empty());
    }

    #[test]
    fn a_photo_counts_only_when_its_title_names_the_query() {
        let names = |query: &str, title: &str| Wanted::new(query).score(Some(title)).is_some();

        assert!(!names("Mac mini M6 2026", "Apple Mac mini - fourth generation"));
        assert!(!names("Mac mini M6 2026", "Moscow, Haval M6 white, May 2026"));
        assert!(names("Mac mini M6 2026", "Apple's Mac mini M6 (2026) review"));
        assert!(!names("Mac mini M4", "Mac mini M40"));
        assert!(!names("Raspberry Pi Pico board", "Raspberry Pi OS 13 screenshot"));
        assert!(names("Raspberry Pi Pico board", "Raspberry Pi Pico top and bottom"));
        assert!(!names("inline fan speed controller cable potentiometer", "2022 Ram Heavy Duty 2500 Tradesman"));
        assert!(!names("4 pin PWM fan speed controller knob", "5mm Cycling RGB LED"));
        assert!(names("4 pin PWM fan speed controller knob", "Amazon.com: 12V 4-Pin PWM Fan Controller, 6 Fans"));
        assert!(names("Belém Tower Lisbon", "Illuminated tower of Belem"));
        assert!(!names("Mac mini", "Meizu M6 miniPlayer"));
        assert!(Wanted::new("Ferry").score(None).is_none());
    }

    #[test]
    fn web_photos_name_the_query_and_show_it() {
        let page = [
            web_result("Fan speed controller wiring diagram", "https://a.example/wiring.jpg", ""),
            web_result("Women's pumps", "https://a.example/pumps.jpg", ""),
            web_result("Inline fan speed controller", "https://a.example/cable.jpg", "https://tse.example/th?id=1"),
        ]
        .concat();

        assert_eq!(
            web_photos(&page, "inline fan speed controller"),
            ["https://a.example/cable.jpg".to_string(), format!("https://tse.example/th?id=1&{WEB_THUMBNAIL_SIZE}")]
        );
        assert_eq!(web_photos(&page, "fan speed controller wiring diagram")[0], "https://a.example/wiring.jpg");
    }

    #[tokio::test]
    async fn the_webs_first_photo_that_names_the_query_is_kept_across_restarts() {
        let (base, asked) = stand_in().await;
        let dir = tempfile::tempdir().unwrap();
        let media = MediaStore::new(dir.path().join("media"), "/");
        let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();

        let found = lookups(&base).images(&media, &store, "  Ferry   Building ").await.unwrap();
        let asked_first = asked.lock().unwrap().len();
        let again = lookups(&base).images(&media, &store, "Ferry Building").await.unwrap();

        let shown: Vec<(&str, Option<u32>)> = found.iter().map(|image| (image.src.as_str(), image.width)).collect();
        let thumbnail = format!("{base}/photos/320px-Ferry.png?{WEB_THUMBNAIL_SIZE}");
        assert_eq!(shown, [(thumbnail.as_str(), Some(320))]);
        assert!(found.iter().all(|image| media.has(&image.id) && !image.video));
        assert_eq!(again, found);
        assert_eq!(asked.lock().unwrap().len(), asked_first, "a kept answer isn't asked again");
        assert_eq!(photos_asked(&asked), ["gone.png", "320px-Ferry.png"], "only the result that names it");
        assert!(!asked.lock().unwrap().iter().any(|asked| asked.starts_with("/w/") || asked.starts_with("/api/")));
    }

    #[tokio::test]
    async fn without_the_web_photos_come_from_wikimedia_when_they_name_the_query() {
        let (base, asked) = stand_in().await;
        let dir = tempfile::tempdir().unwrap();
        let media = MediaStore::new(dir.path().join("media"), "/");
        let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();
        let offline = Lookups::new("http://127.0.0.1:1", &base, &base, &base);

        assert_eq!(
            offline.wikimedia_photos("Ferry Building").await.unwrap(),
            [
                format!("{base}/photos/320px-Ferry.png"),
                format!("{base}/photos/Inside.png"),
                "https://127.0.0.1:1/commons.png".to_string(),
            ]
        );
        assert!(offline.article_photos("Ferry terminal").await.unwrap().is_empty(), "it leads to another article");
        let found = offline.images(&media, &store, "Ferry Building").await.unwrap();
        assert_eq!(found[0].src, format!("{base}/photos/320px-Ferry.png"));
        assert!(asked.lock().unwrap().iter().any(|asked| asked.starts_with("/api/")));
    }

    #[tokio::test]
    async fn a_failed_lookup_is_asked_again_and_a_route_is_kept() {
        let (base, asked) = stand_in().await;
        let dir = tempfile::tempdir().unwrap();
        let media = MediaStore::new(dir.path().join("media"), "/");
        let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();
        let lookups = lookups(&base);

        assert!(lookups.images(&media, &store, "Broken thing").await.is_err());
        let after_failure = asked.lock().unwrap().len();
        assert!(lookups.images(&media, &store, "Broken thing").await.is_err());
        assert!(asked.lock().unwrap().len() > after_failure, "a failure isn't kept");

        let stops = [[37.79, -122.39], [37.8, -122.4]];
        assert_eq!(lookups.route(&stops).await.unwrap(), [[37.79, -122.39], [37.8, -122.4]]);
        let routed = asked.lock().unwrap().len();
        assert_eq!(lookups.route(&stops).await.unwrap().len(), 2);
        assert_eq!(asked.lock().unwrap().len(), routed);
        assert!(lookups.route(&[[1.0, 2.0], [3.0, 4.0]]).await.is_err());
        assert!(lookups.route(&[[1.0, 2.0]]).await.is_err());
        assert!(lookups.route(&[[91.0, 2.0], [3.0, 4.0]]).await.is_err());
    }
}
