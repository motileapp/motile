//! OpenFreeMap's vector tiles: which cover a map, decoded from Mapbox Vector Tiles into what the
//! style draws, and the store every map shares. The store fetches them off the main thread with
//! a Motile User-Agent, keeps the decoded ones in memory (a bounded LRU) and the raw ones on disk
//! in the app's data folder (bounded too), and tells the maps when one arrives.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use motile_core::render::openui::map::{Camera, TILE, world};
use serde_json::Value;

use super::style::Style;

/// The deepest zoom OpenFreeMap has tiles for; deeper zooms draw these larger.
pub const MAX_ZOOM: u8 = 14;
const TILEJSON: &str = "https://tiles.openfreemap.org/planet";
const DECODED: usize = 40;
const DISK_LIMIT: u64 = 150 * 1024 * 1024;
const DISK_STALE: Duration = Duration::from_secs(30 * 24 * 3600);
const RETRY: Duration = Duration::from_secs(30);
const AT_ONCE: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileId {
    pub fn parent(self) -> Option<TileId> {
        (self.z > 0).then(|| TileId { z: self.z - 1, x: self.x / 2, y: self.y / 2 })
    }
}

/// A decoded tile: its layers by name, with the features' coordinates 0–1 across the tile.
#[derive(Default, Debug)]
pub struct Tile {
    pub layers: HashMap<String, Vec<Feature>>,
}

#[derive(Debug)]
pub struct Feature {
    pub props: Vec<(String, Value)>,
    pub geometry: Geometry,
    /// The corners of what it covers: min x, min y, max x, max y.
    pub bounds: [f32; 4],
}

#[derive(Debug)]
pub enum Geometry {
    Points(Vec<[f32; 2]>),
    Lines(Vec<Vec<[f32; 2]>>),
    /// Every ring of every polygon, outer ones and holes, which are filled even-odd.
    Polygons(Vec<Vec<[f32; 2]>>),
}

/// Decodes a tile's bytes, gzipped or not, keeping the layers `style` draws and the properties
/// it reads.
pub fn decode(bytes: &[u8], style: &Style) -> anyhow::Result<Tile> {
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(bytes), &mut out)?;
        out
    } else {
        bytes.to_vec()
    };
    if bytes.is_empty() {
        return Ok(Tile::default());
    }
    let reader = mvt_reader::Reader::new(bytes).map_err(|error| anyhow::anyhow!("{error}"))?;
    let wanted: HashSet<&str> = style
        .layers
        .iter()
        .filter_map(|layer| match &layer.source {
            super::style::Source::Tiles(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let mut layers = HashMap::new();
    for meta in reader.get_layer_metadata().map_err(|error| anyhow::anyhow!("{error}"))? {
        if !wanted.contains(meta.name.as_str()) {
            continue;
        }
        let extent = meta.extent.max(1) as f32;
        let features = reader.get_features(meta.layer_index).map_err(|error| anyhow::anyhow!("{error}"))?;
        let features = features.into_iter().filter_map(|feature| convert(feature, extent, &style.keys)).collect();
        layers.insert(meta.name, features);
    }
    Ok(Tile { layers })
}

fn convert(feature: mvt_reader::feature::Feature, extent: f32, keys: &[String]) -> Option<Feature> {
    use geo_types::Geometry as Geo;
    let point = |coord: geo_types::Coord<f32>| [coord.x / extent, coord.y / extent];
    let ring = |line: &geo_types::LineString<f32>| line.0.iter().map(|coord| point(*coord)).collect::<Vec<_>>();
    let geometry = match &feature.geometry {
        Geo::Point(at) => Geometry::Points(vec![point(at.0)]),
        Geo::MultiPoint(points) => Geometry::Points(points.iter().map(|at| point(at.0)).collect()),
        Geo::LineString(line) => Geometry::Lines(vec![ring(line)]),
        Geo::MultiLineString(lines) => Geometry::Lines(lines.iter().map(ring).collect()),
        Geo::Polygon(polygon) => {
            Geometry::Polygons(std::iter::once(polygon.exterior()).chain(polygon.interiors()).map(ring).collect())
        }
        Geo::MultiPolygon(polygons) => Geometry::Polygons(
            polygons
                .iter()
                .flat_map(|polygon| std::iter::once(polygon.exterior()).chain(polygon.interiors()))
                .map(ring)
                .collect(),
        ),
        _ => return None,
    };
    let props = feature
        .properties
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| keys.contains(key))
        .map(|(key, value)| (key, json(value)))
        .collect();
    let bounds = bounds(&geometry);
    Some(Feature { props, geometry, bounds })
}

fn json(value: mvt_reader::feature::Value) -> Value {
    use mvt_reader::feature::Value as Mvt;
    match value {
        Mvt::String(text) => text.into(),
        Mvt::Float(number) => (number as f64).into(),
        Mvt::Double(number) => number.into(),
        Mvt::Int(number) | Mvt::SInt(number) => number.into(),
        Mvt::UInt(number) => number.into(),
        Mvt::Bool(on) => on.into(),
        Mvt::Null => Value::Null,
    }
}

fn bounds(geometry: &Geometry) -> [f32; 4] {
    let points: Box<dyn Iterator<Item = &[f32; 2]>> = match geometry {
        Geometry::Points(points) => Box::new(points.iter()),
        Geometry::Lines(parts) | Geometry::Polygons(parts) => Box::new(parts.iter().flatten()),
    };
    points.fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |[x0, y0, x1, y1], [x, y]| {
        [x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y)]
    })
}

/// A tile the map shows, where it lies: `x` is unwrapped, so that a map across the antimeridian
/// draws the world's copies side by side.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Slot {
    pub z: u8,
    pub x: i64,
    pub y: u32,
}

impl Slot {
    pub fn id(self) -> TileId {
        TileId { z: self.z, x: self.x.rem_euclid(1 << self.z) as u32, y: self.y }
    }

    /// The slot of the tile `levels` above it, which can stand in for it.
    pub fn up(self, levels: u8) -> Slot {
        Slot { z: self.z - levels, x: self.x.div_euclid(1 << levels), y: self.y >> levels }
    }

    /// Its left, top, right and bottom in points, in a map of this size at this camera.
    pub fn rect(self, camera: Camera, width: f64, height: f64) -> [f64; 4] {
        let size = TILE / f64::from(1u32 << self.z);
        let center = world(camera.lat, camera.lng);
        let scale = camera.zoom.exp2();
        let left = (self.x as f64 * size - center[0]) * scale + width / 2.;
        let top = (f64::from(self.y) * size - center[1]) * scale + height / 2.;
        [left, top, left + size * scale, top + size * scale]
    }
}

/// The level of tiles a map at this zoom draws, as MapLibre picks it for 512-point tiles.
pub fn level(zoom: f64) -> u8 {
    zoom.floor().clamp(0., f64::from(MAX_ZOOM)) as u8
}

/// The tiles that cover a map this many points wide and tall.
pub fn cover(camera: Camera, width: f64, height: f64) -> Vec<Slot> {
    let z = level(camera.zoom);
    let count = f64::from(1u32 << z);
    let center = world(camera.lat, camera.lng);
    let scale = camera.zoom.exp2();
    let per_tile = TILE / count;
    let to_tile = |world: f64| world / per_tile;
    let (left, right) = (to_tile(center[0] - width / 2. / scale), to_tile(center[0] + width / 2. / scale));
    let (top, bottom) = (to_tile(center[1] - height / 2. / scale), to_tile(center[1] + height / 2. / scale));
    let rows = (top.floor().max(0.) as u32)..=(bottom.ceil().min(count) as u32).saturating_sub(1);
    let columns = (left.floor() as i64)..(right.ceil() as i64);
    rows.flat_map(|y| columns.clone().map(move |x| Slot { z, x, y })).collect()
}

/// What draws in a slot: its own tile, or the nearest above it the store has, which the
/// painter clips to the slot.
pub fn stand_ins(slots: &[Slot], has: impl Fn(TileId) -> bool) -> Vec<(Slot, Slot)> {
    slots
        .iter()
        .filter_map(|slot| (0..=slot.z).map(|levels| slot.up(levels)).find(|up| has(up.id())).map(|up| (*slot, up)))
        .collect()
}

/// The tiles every map shares.
pub struct Store {
    state: Mutex<State>,
}

struct State {
    decoded: lru::LruCache<TileId, Arc<Tile>>,
    /// What each live map wants, by the map.
    wanted: HashMap<u64, Vec<TileId>>,
    pending: HashSet<TileId>,
    failed: HashMap<TileId, Instant>,
    failing: bool,
    template: Option<String>,
    listeners: Vec<UnboundedSender<()>>,
    worker: Option<UnboundedSender<TileId>>,
}

static STORE: LazyLock<Store> = LazyLock::new(|| Store {
    state: Mutex::new(State {
        decoded: lru::LruCache::new(NonZeroUsize::new(DECODED).unwrap_or(NonZeroUsize::MIN)),
        wanted: HashMap::new(),
        pending: HashSet::new(),
        failed: HashMap::new(),
        failing: false,
        template: None,
        listeners: Vec::new(),
        worker: None,
    }),
});

pub fn store() -> &'static Store {
    &STORE
}

impl Store {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The tile, when it is decoded.
    pub fn tile(&self, id: TileId) -> Option<Arc<Tile>> {
        self.state().decoded.get(&id).cloned()
    }

    pub fn has(&self, id: TileId) -> bool {
        self.state().decoded.contains(&id)
    }

    /// Whether tiles failed to come lately: the error box shows.
    pub fn failing(&self) -> bool {
        self.state().failing
    }

    /// A stream that says when a tile came or the failing changed.
    pub fn listen(&self) -> UnboundedReceiver<()> {
        let (sender, receiver) = unbounded();
        self.state().listeners.push(sender);
        receiver
    }

    /// The map `owner` shows these tiles now: they are fetched unless they are here or coming.
    pub fn want(&self, owner: u64, ids: Vec<TileId>) {
        let mut state = self.state();
        let missing: Vec<TileId> = ids
            .iter()
            .copied()
            .filter(|id| !state.decoded.contains(id) && !state.pending.contains(id))
            .filter(|id| state.failed.get(id).is_none_or(|at| at.elapsed() >= RETRY))
            .collect();
        state.wanted.insert(owner, ids);
        if missing.is_empty() {
            return;
        }
        let worker = state.worker.get_or_insert_with(spawn_worker).clone();
        for id in missing {
            state.pending.insert(id);
            if worker.unbounded_send(id).is_err() {
                state.worker = None;
                state.pending.remove(&id);
            }
        }
    }

    /// The map no longer shows anything: its tiles can go.
    pub fn release(&self, owner: u64) {
        self.state().wanted.remove(&owner);
    }

    fn still_wanted(&self, id: TileId) -> bool {
        let mut state = self.state();
        let wanted = state.wanted.values().any(|ids| ids.contains(&id));
        if !wanted {
            state.pending.remove(&id);
        }
        wanted
    }

    fn landed(&self, id: TileId, tile: anyhow::Result<Tile>) {
        let mut state = self.state();
        state.pending.remove(&id);
        match tile {
            Ok(tile) => {
                state.decoded.put(id, Arc::new(tile));
                state.failed.remove(&id);
                state.failing = false;
            }
            Err(error) => {
                tracing::warn!("couldn't load map tile {}/{}/{}: {error:#}", id.z, id.x, id.y);
                state.failed.insert(id, Instant::now());
                state.failing = true;
            }
        }
        state.listeners.retain(|listener| listener.unbounded_send(()).is_ok());
    }
}

/// `Motile/<version>`, which OpenFreeMap is told the tiles are for.
pub fn user_agent() -> String {
    format!("Motile/{}", crate::updater::release_version())
}

fn spawn_worker() -> UnboundedSender<TileId> {
    let (sender, receiver) = unbounded();
    let started = std::thread::Builder::new().name("map-tiles".into()).spawn(move || {
        motile_protocol::tls::install();
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        runtime.block_on(work(receiver));
    });
    if let Err(error) = started {
        tracing::warn!("couldn't start fetching map tiles: {error}");
    }
    sender
}

async fn work(receiver: UnboundedReceiver<TileId>) {
    use futures::StreamExt;
    let Ok(client) = reqwest::Client::builder().user_agent(user_agent()).timeout(Duration::from_secs(20)).build()
    else {
        return;
    };
    let folder = crate::store::data_dir().join("map-tiles");
    let _ = tokio::task::spawn_blocking({
        let folder = folder.clone();
        move || prune(&folder, DISK_LIMIT)
    })
    .await;
    receiver
        .for_each_concurrent(AT_ONCE, |id| {
            let (client, folder) = (&client, &folder);
            async move {
                if !store().still_wanted(id) {
                    return;
                }
                let tile = load(client, folder, id).await;
                store().landed(id, tile);
            }
        })
        .await;
}

async fn load(client: &reqwest::Client, folder: &std::path::Path, id: TileId) -> anyhow::Result<Tile> {
    let file = folder.join(format!("{}/{}/{}.pbf", id.z, id.x, id.y));
    let cached = tokio::fs::metadata(&file)
        .await
        .ok()
        .and_then(|meta| meta.modified().ok())
        .is_some_and(|modified| SystemTime::now().duration_since(modified).unwrap_or_default() < DISK_STALE);
    let bytes = match cached {
        true => tokio::fs::read(&file).await?,
        false => {
            let bytes = fetch(client, id).await?;
            if let Some(parent) = file.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(&file, &bytes).await;
            bytes
        }
    };
    tokio::task::spawn_blocking(move || decode(&bytes, super::style::style(true))).await?
}

async fn fetch(client: &reqwest::Client, id: TileId) -> anyhow::Result<Vec<u8>> {
    let template = template(client).await?;
    let url =
        template.replace("{z}", &id.z.to_string()).replace("{x}", &id.x.to_string()).replace("{y}", &id.y.to_string());
    let response = client.get(&url).send().await?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    anyhow::ensure!(status.is_success(), "{url} answered {status}");
    Ok(response.bytes().await?.to_vec())
}

/// The tiles' address, from OpenFreeMap's TileJSON, asked once.
async fn template(client: &reqwest::Client) -> anyhow::Result<String> {
    if let Some(template) = store().state().template.clone() {
        return Ok(template);
    }
    let tilejson: Value = client.get(TILEJSON).send().await?.error_for_status()?.json().await?;
    let template =
        tilejson["tiles"][0].as_str().ok_or_else(|| anyhow::anyhow!("the TileJSON names no tiles"))?.to_owned();
    store().state().template = Some(template.clone());
    Ok(template)
}

/// Deletes the oldest tiles on disk until they take less than `limit`.
fn prune(folder: &std::path::Path, limit: u64) {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
                continue;
            }
            files.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len(), entry.path()));
        }
    }
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    if total <= limit {
        return;
    }
    files.sort();
    for (_, size, path) in files {
        if total <= limit * 4 / 5 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// San Francisco from OpenFreeMap's planet of 2026-10-11 (© OpenMapTiles, data © OpenStreetMap
    /// contributors under the ODbL).
    pub const FIXTURES: [(TileId, &[u8]); 4] = [
        (TileId { z: 12, x: 654, y: 1582 }, include_bytes!("fixtures/12-654-1582.pbf")),
        (TileId { z: 12, x: 655, y: 1582 }, include_bytes!("fixtures/12-655-1582.pbf")),
        (TileId { z: 12, x: 654, y: 1583 }, include_bytes!("fixtures/12-654-1583.pbf")),
        (TileId { z: 12, x: 655, y: 1583 }, include_bytes!("fixtures/12-655-1583.pbf")),
    ];

    pub fn fixtures() -> HashMap<TileId, Arc<Tile>> {
        FIXTURES
            .iter()
            .map(|(id, bytes)| (*id, Arc::new(decode(bytes, super::super::style::style(true)).unwrap())))
            .collect()
    }

    #[test]
    fn san_francisco_decodes() {
        let tiles = fixtures();
        let tile = &tiles[&TileId { z: 12, x: 655, y: 1583 }];
        let mut names: Vec<&str> = tile.layers.keys().map(String::as_str).collect();
        names.sort();
        assert_eq!(names, ["landcover", "landuse", "park", "place", "transportation", "water", "waterway"]);
        let water = &tile.layers["water"];
        assert!(water.iter().all(|feature| matches!(feature.geometry, Geometry::Polygons(_))));
        let roads = &tile.layers["transportation"];
        assert!(roads.iter().all(|feature| matches!(feature.geometry, Geometry::Lines(_))));
        assert!(
            roads.iter().any(|feature| feature.props.iter().any(|(key, value)| key == "class" && value == "primary"))
        );
        let places: Vec<&str> = tiles
            .values()
            .flat_map(|tile| &tile.layers["place"])
            .filter_map(|feature| feature.props.iter().find(|(key, _)| key == "name_en")?.1.as_str())
            .collect();
        assert!(places.contains(&"San Francisco"), "{places:?}");
        for feature in water {
            let [x0, y0, x1, y1] = feature.bounds;
            assert!(x0 >= -0.1 && y0 >= -0.1 && x1 <= 1.1 && y1 <= 1.1, "{:?}", feature.bounds);
        }
    }

    #[test]
    fn gzipped_and_empty_tiles_decode() {
        use std::io::Write;
        let style = super::super::style::style(false);
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(FIXTURES[0].1).unwrap();
        let tile = decode(&gzip.finish().unwrap(), style).unwrap();
        assert_eq!(tile.layers.len(), decode(FIXTURES[0].1, style).unwrap().layers.len());
        assert!(decode(&[], style).unwrap().layers.is_empty());
        assert!(decode(&[0xff, 0xff, 0xff], style).is_err());
    }

    #[test]
    fn the_cover_is_the_tiles_on_screen() {
        let camera = Camera { lat: 37.8077, lng: -122.4360, zoom: 12.225 };
        let slots = cover(camera, 720., 400.);
        let ids: Vec<TileId> = slots.iter().map(|slot| slot.id()).collect();
        assert_eq!(ids, FIXTURES.iter().map(|(id, _)| *id).collect::<Vec<_>>());
        let deep = cover(Camera { zoom: 16.5, ..camera }, 720., 400.);
        assert!(deep.iter().all(|slot| slot.z == MAX_ZOOM));
        let world = cover(Camera { lat: 0., lng: 179.9, zoom: 0.5 }, 720., 400.);
        assert!(world.iter().any(|slot| slot.x == 1) && world.iter().all(|slot| slot.id().x == 0));
    }

    #[test]
    fn a_parent_stands_in_while_its_child_loads() {
        let slots = [Slot { z: 14, x: 2620, y: 6331 }, Slot { z: 14, x: 2621, y: 6331 }];
        let loaded = [TileId { z: 12, x: 655, y: 1582 }, TileId { z: 14, x: 2621, y: 6331 }];
        let picked = stand_ins(&slots, |id| loaded.contains(&id));
        assert_eq!(picked, [(slots[0], Slot { z: 12, x: 655, y: 1582 }), (slots[1], slots[1])]);
        assert!(stand_ins(&slots, |_| false).is_empty());
        let west = Slot { z: 3, x: -1, y: 2 };
        assert_eq!((west.id().x, west.up(1)), (7, Slot { z: 2, x: -1, y: 1 }));
    }

    #[test]
    fn the_disk_keeps_the_newest_tiles() {
        let folder = std::env::temp_dir().join(format!("motile-map-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("12/1")).unwrap();
        for index in 0..10 {
            let file = folder.join(format!("12/1/{index}.pbf"));
            std::fs::write(&file, vec![0u8; 100]).unwrap();
            let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + index * 60);
            std::fs::File::options().write(true).open(&file).unwrap().set_modified(at).unwrap();
        }
        prune(&folder, 500);
        let mut left: Vec<String> = std::fs::read_dir(folder.join("12/1"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["6.pbf", "7.pbf", "8.pbf", "9.pbf"]);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
