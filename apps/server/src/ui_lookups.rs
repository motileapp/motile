//! What programs look up outside, through your server: photos of what a query names, from
//! Wikipedia and Wikimedia Commons as open-intelligent-ui finds them, kept in the media store, and
//! the streets a car takes between stops, from OSRM's public server. Photos are kept in the store
//! and routes for a while; failures are not.

use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, bail};
use lru::LruCache;
use motile_protocol::wire::Media;
use serde_json::Value;

use tokio::sync::Semaphore;

use crate::media::{MediaStore, percent_decode};
use crate::store::Store;

const USER_AGENT: &str = concat!("Motile/", env!("CARGO_PKG_VERSION"), " (https://motile.app)");
const WIKI_TIMEOUT: Duration = Duration::from_secs(8);
const ROUTE_TIMEOUT: Duration = Duration::from_secs(10);
const PHOTO_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_PHOTO: usize = 10 * 1024 * 1024;
const LOOKUPS_AT_ONCE: usize = 4;
const KEPT_ROUTES: usize = 200;
const MAX_STOPS: usize = 25;
const THUMBNAIL_WIDTH: &str = "960";
/// Photos whose names say they are none.
const NOT_PHOTOS: [&str; 6] = [".svg", "icon", "logo", "map", "flag", "seal"];
const NOT_SEARCHED_PHOTOS: [&str; 3] = ["coat_of_arms", "signature", "diagram"];

pub struct Lookups {
    http: reqwest::Client,
    wikipedia: String,
    commons: String,
    osrm: String,
    /// Wikipedia slows down a burst of lookups, so only a few run at once.
    at_once: Semaphore,
    routes: Mutex<LruCache<String, Vec<[f64; 2]>>>,
}

impl Default for Lookups {
    fn default() -> Self {
        Self::new("https://en.wikipedia.org", "https://commons.wikimedia.org", "https://router.project-osrm.org")
    }
}

impl Lookups {
    pub fn new(wikipedia: &str, commons: &str, osrm: &str) -> Self {
        motile_protocol::tls::install();
        let http = reqwest::Client::builder().user_agent(USER_AGENT).build().unwrap_or_default();
        let kept = |count| NonZeroUsize::new(count).unwrap_or(NonZeroUsize::MIN);
        Self {
            http,
            wikipedia: wikipedia.to_string(),
            commons: commons.to_string(),
            osrm: osrm.to_string(),
            at_once: Semaphore::new(LOOKUPS_AT_ONCE),
            routes: Mutex::new(LruCache::new(kept(KEPT_ROUTES))),
        }
    }

    /// The photo of what `query` names: the article with that title's, else a search's. Only one
    /// is shown, so only the best that downloads is fetched.
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
        let titled = async {
            let urls = self.article_photos(&query).await?;
            anyhow::Ok((self.first_photo(media, &urls).await, urls.is_empty()))
        };
        let searching = self.searched_photos(&query);
        tokio::pin!(titled, searching);
        let mut searched = None;
        let titled = loop {
            tokio::select! {
                titled = &mut titled => break titled,
                found = &mut searching, if searched.is_none() => searched = Some(found),
            }
        };
        if let Ok((Some(photo), _)) = titled {
            store.keep_ui_photos(&query, std::slice::from_ref(&photo))?;
            return Ok(vec![photo]);
        }
        let searched = match searched {
            Some(searched) => searched,
            None => searching.await,
        };
        let photo = match &searched {
            Ok(urls) => self.first_photo(media, urls).await,
            Err(_) => None,
        };
        let images: Vec<Media> = photo.into_iter().collect();
        let none_found = matches!(titled, Ok((None, true))) && searched.is_ok_and(|urls| urls.is_empty());
        if images.is_empty() && !none_found {
            bail!("Wikipedia couldn't be reached.");
        }
        if titled.is_ok() {
            store.keep_ui_photos(&query, &images)?;
        }
        Ok(images)
    }

    /// The first of the photos that downloads, kept in the media store.
    async fn first_photo(&self, media: &MediaStore, urls: &[String]) -> Option<Media> {
        for url in urls {
            if let Some(photo) = download(&self.http, media.clone(), url).await {
                return Some(photo);
            }
        }
        None
    }

    /// The lead photo of the Wikipedia article with that title, then up to two more of its photos.
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
        Ok(article_photos(&summary?, &media?))
    }

    /// What the article's address answers; nothing for an article that isn't there.
    async fn article(&self, url: reqwest::Url) -> anyhow::Result<Value> {
        let answer = self.http.get(url).timeout(WIKI_TIMEOUT).send().await?;
        if answer.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }
        if !answer.status().is_success() {
            bail!("Wikipedia answered {}.", answer.status());
        }
        Ok(answer.json().await?)
    }

    /// The lead photos of the articles about the query, then Commons' photos, at most four.
    async fn searched_photos(&self, query: &str) -> anyhow::Result<Vec<String>> {
        let mut photos = Vec::new();
        let articles = self
            .search(
                &self.wikipedia,
                &[
                    ("generator", "search"),
                    ("gsrsearch", query),
                    ("gsrlimit", "6"),
                    ("prop", "pageimages"),
                    ("piprop", "thumbnail"),
                    ("pithumbsize", THUMBNAIL_WIDTH),
                ],
            )
            .await?;
        // Only articles about the query: a dish's search shouldn't show a restaurant chain's storefront.
        let wanted = words(query);
        let about =
            |page: &&Value| words(page["title"].as_str().unwrap_or_default()).iter().any(|word| wanted.contains(word));
        for page in articles.iter().filter(about) {
            add_searched(&mut photos, page["thumbnail"]["source"].as_str());
        }
        for terms in tries(query) {
            if photos.len() >= 3 {
                break;
            }
            let search = format!("{terms} filetype:bitmap");
            let files = self
                .search(
                    &self.commons,
                    &[
                        ("generator", "search"),
                        ("gsrsearch", &search),
                        ("gsrnamespace", "6"),
                        ("gsrlimit", "6"),
                        ("prop", "imageinfo"),
                        ("iiprop", "url|mime"),
                        ("iiurlwidth", THUMBNAIL_WIDTH),
                    ],
                )
                .await?;
            for file in &files {
                let info = &file["imageinfo"][0];
                if info["mime"].as_str().is_some_and(|mime| mime.starts_with("image/")) {
                    add_searched(&mut photos, info["thumburl"].as_str().or(info["url"].as_str()));
                }
            }
        }
        photos.truncate(4);
        Ok(photos)
    }

    /// The pages a MediaWiki query answers with, in the order it ranks them.
    async fn search(&self, site: &str, parameters: &[(&str, &str)]) -> anyhow::Result<Vec<Value>> {
        let url = format!("{site}/w/api.php");
        let fixed = [("action", "query"), ("format", "json"), ("formatversion", "2")];
        let asked = self.http.get(url).query(&fixed).query(parameters).timeout(WIKI_TIMEOUT);
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
    photos.extend(summary["thumbnail"]["source"].as_str().map(str::to_string));
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

/// The words that say what a photo shows: those longer than three letters.
fn words(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let words = lower.split(|letter: char| !letter.is_alphanumeric()).filter(|word| word.chars().count() > 3);
    words.map(str::to_string).collect()
}

/// The query, then its first three and its first two words, each once.
fn tries(query: &str) -> Vec<String> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    let mut tries: Vec<String> = Vec::new();
    for count in [terms.len(), 3, 2] {
        let tried = terms[..count.min(terms.len())].join(" ");
        if !tries.contains(&tried) {
            tries.push(tried);
        }
    }
    tries
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

    /// Wikipedia, Commons and OSRM on one local address, and what each was asked.
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
                    "/api/rest_v1/page/summary/Ferry_Building" => {
                        json(json!({ "thumbnail": { "source": format!("{base}/photos/320px-Ferry.png") } }))
                    }
                    "/api/rest_v1/page/media-list/Ferry_Building" => json(json!({ "items": [
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/640px-Ferry.png") }] },
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/Map_of_the_pier.png") }] },
                        { "type": "video", "srcset": [{ "src": format!("{base}/photos/tour.png") }] },
                        { "type": "image", "srcset": [{ "src": format!("{base}/photos/Inside.png") }] },
                    ] })),
                    "/w/api.php" if query.contains("Broken") => StatusCode::BAD_GATEWAY.into_response(),
                    "/w/api.php" if query.contains("gsrnamespace=6") => json(json!({ "query": { "pages": [
                        { "index": 1, "imageinfo": [{ "mime": "image/png", "thumburl": "https://127.0.0.1:1/commons.png" }] },
                        { "index": 2, "imageinfo": [{ "mime": "image/svg+xml", "url": "https://127.0.0.1:1/plan.svg" }] },
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
        assert!(article_photos(&Value::Null, &Value::Null).is_empty());
    }

    #[test]
    fn a_long_query_is_tried_shorter_and_only_long_words_count() {
        assert_eq!(tries("old stone bridge at dusk"), ["old stone bridge at dusk", "old stone bridge", "old stone"]);
        assert_eq!(tries("Lisbon"), ["Lisbon"]);
        assert_eq!(words("Belém Tower, at dusk"), ["belém", "tower", "dusk"]);
    }

    #[tokio::test]
    async fn only_the_best_photo_is_fetched_and_kept_across_restarts() {
        let (base, asked) = stand_in().await;
        let dir = tempfile::tempdir().unwrap();
        let media = MediaStore::new(dir.path().join("media"), "/");
        let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();

        let found = Lookups::new(&base, &base, &base).images(&media, &store, "  Ferry   Building ").await.unwrap();
        let asked_first = asked.lock().unwrap().len();
        let again = Lookups::new(&base, &base, &base).images(&media, &store, "Ferry Building").await.unwrap();

        let shown: Vec<(&str, Option<u32>)> = found.iter().map(|image| (image.src.as_str(), image.width)).collect();
        assert_eq!(shown, [(format!("{base}/photos/320px-Ferry.png").as_str(), Some(320))]);
        assert!(found.iter().all(|image| media.has(&image.id) && !image.video));
        assert_eq!(again, found);
        assert_eq!(asked.lock().unwrap().len(), asked_first, "a kept answer isn't asked again");
        let downloaded = asked.lock().unwrap().iter().filter(|asked| asked.starts_with("/photos/")).count();
        assert_eq!(downloaded, 1);
    }

    #[tokio::test]
    async fn a_failed_lookup_is_asked_again_and_a_route_is_kept() {
        let (base, asked) = stand_in().await;
        let dir = tempfile::tempdir().unwrap();
        let media = MediaStore::new(dir.path().join("media"), "/");
        let store = Store::open(&dir.path().join("motile.sqlite")).unwrap();
        let lookups = Lookups::new(&base, &base, &base);

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
