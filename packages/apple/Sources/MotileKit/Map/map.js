// The Mac app's side of the map in MapLibre GL JS: Swift calls `motile`, and hears back through
// the `map` message handler.
(() => {
  const post = (message) => window.webkit.messageHandlers.map.postMessage(message);
  const empty = { type: "FeatureCollection", features: [] };
  const images = new Map();
  let map = null;
  let styled = false;
  let pins = empty;
  let route = empty;
  let narrow = false;
  let failing = false;

  const bytes = (base64) => Uint8Array.from(atob(base64), (character) => character.charCodeAt(0));

  const label = () => {
    if (!styled || !map.getLayer("pin-label")) return;
    map.setLayoutProperty("pin-label", "text-size", narrow ? 10 : 12);
    map.setLayoutProperty("pin-label", "text-max-width", narrow ? 12 : 13);
  };

  const apply = () => {
    if (!styled) return;
    for (const [id, image] of images) {
      if (!map.hasImage(id)) map.addImage(id, image.data, { pixelRatio: image.ratio });
    }
    map.getSource("pins")?.setData(pins);
    map.getSource("route")?.setData(route);
    label();
  };

  const rest = () => {
    const center = map.getCenter();
    const points = pins.features.map((feature) => {
      const point = map.project(feature.geometry.coordinates);
      return { key: feature.properties.key, x: point.x, y: point.y };
    });
    post({ type: "rest", lat: center.lat, lng: center.lng, zoom: map.getZoom(), pins: points });
  };

  const tiles = (failed) => {
    if (failing === failed) return;
    failing = failed;
    post({ type: "tiles", failing: failed });
  };

  const pinAt = (point) => map.queryRenderedFeatures(point, { layers: ["pin"] })[0];

  window.motile = {
    start(style, camera) {
      map = new maplibregl.Map({
        container: "map",
        style,
        center: [camera.lng, camera.lat],
        zoom: camera.zoom,
        minZoom: 0,
        maxZoom: 20,
        maxPitch: 0,
        scrollZoom: false,
        boxZoom: false,
        dragRotate: false,
        pitchWithRotate: false,
        touchPitch: false,
        attributionControl: false,
      });
      map.touchZoomRotate.disableRotation();
      map.keyboard.disableRotation();
      map.on("style.load", () => {
        styled = true;
        apply();
      });
      map.once("load", () => post({ type: "loaded" }));
      map.on("movestart", (event) => {
        if (event.originalEvent) post({ type: "moved" });
      });
      map.on("moveend", rest);
      map.on("click", (event) => {
        const pin = pinAt(event.point);
        if (pin) post({ type: "pin", key: pin.properties.key, label: pin.properties.label });
      });
      map.on("mousemove", (event) => {
        map.getCanvas().style.cursor = pinAt(event.point) ? "pointer" : "";
      });
      map.on("styleimagemissing", (event) => {
        if (!map.hasImage(event.id)) map.addImage(event.id, { width: 1, height: 1, data: new Uint8Array(4) });
      });
      map.on("error", (event) => {
        if (event.sourceId === "pins" || event.sourceId === "route") return;
        tiles(true);
      });
      map.on("sourcedata", (event) => {
        if (event.sourceId === "openmaptiles" && event.tile?.state === "loaded") tiles(false);
      });
    },
    style(style) {
      styled = false;
      map.setStyle(style, { diff: false });
    },
    pins(data, added) {
      for (const image of added) {
        images.set(image.id, { data: { width: image.side, height: image.side, data: bytes(image.rgba) }, ratio: image.ratio });
      }
      pins = data;
      apply();
      if (map) rest();
    },
    line(data) {
      route = data;
      if (styled) map.getSource("route")?.setData(route);
    },
    jump(camera) {
      map.jumpTo({ center: [camera.lng, camera.lat], zoom: camera.zoom });
    },
    fly(camera, duration) {
      map.flyTo({ center: [camera.lng, camera.lat], zoom: camera.zoom, duration, essential: true });
    },
    zoom(by, x, y, duration) {
      const options = { duration };
      if (x !== null) options.around = map.unproject([x, y]);
      map.zoomTo(map.getZoom() + by, options);
    },
    narrow(on) {
      narrow = on;
      label();
    },
  };
  post({ type: "ready" });
})();
