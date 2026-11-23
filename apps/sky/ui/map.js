// The map page: base map, flown track and own position.
//
// The map only draws. Position and track come from Rust (see app.js), so nothing is
// lost while the web view is paused with the screen locked: after unlocking, the
// missing track points are simply fetched.

"use strict";

const FlightMap = (() => {
  // Shown when the configured style cannot be loaded (e.g. no network): a plain
  // background, so that track and position are still visible.
  const emptyStyle = () => ({
    version: 8,
    sources: {},
    layers: [
      { id: "background", type: "background", paint: { "background-color": themeColors().fallback } },
    ],
  });
  const TRACK_SOURCE = "track";
  const SAVED_SOURCE = "saved-flight";
  const AIRSPACE_SOURCE = "airspace";

  // Airspace colours by category (set in Rust, see airspace.rs), after the usual chart
  // conventions: red for restricted and control zones, blue for controlled airspace,
  // orange for danger areas, green for areas reserved for gliding and paragliding.
  const AIRSPACE_COLORS = {
    ctr: "#d4145a",
    controlled: "#1f4fbf",
    classE: "#5a7fc0",
    restricted: "#c01818",
    danger: "#d46a00",
    prohibited: "#7a0000",
    tmz: "#6a3fa0",
    rmz: "#2e7f8b",
    gliderSector: "#a68400",
    reserved: "#1d8a3a",
    other: "#808080",
  };
  // The same hue per category, bright enough to read on a black map.
  const AIRSPACE_COLORS_DARK = {
    ctr: "#ff5c9a",
    controlled: "#6f9bff",
    classE: "#8fb0e8",
    restricted: "#ff6b6b",
    danger: "#ff9a33",
    prohibited: "#e04a4a",
    tmz: "#b58cf0",
    rmz: "#4fc4d4",
    gliderSector: "#e6c200",
    reserved: "#4fd873",
    other: "#a0a0a0",
  };
  // What the map draws itself, for the light and the dark look.
  const OWN_COLORS = {
    light: {
      airspace: AIRSPACE_COLORS,
      fallback: "#e8e8e8",
      halo: "#fff",
      casing: "#fff",
      track: "#c2185b",
      saved: "#1a4fd6",
      alarm: "#d7191c",
      near: "#f28e1c",
    },
    dark: {
      airspace: AIRSPACE_COLORS_DARK,
      fallback: "#000000",
      halo: "#000",
      casing: "#000",
      track: "#ff5ca8",
      saved: "#6f9bff",
      alarm: "#ff6b6b",
      near: "#ffb04d",
    },
  };
  // The layers of the relief, as in `RELIEF_LAYERS` of core-maps.
  const RELIEF_LAYERS = ["hillshade"];
  const themeColors = () => OWN_COLORS[look.dark ? "dark" : "light"];
  // An online style stays light in the dark look, so what lies on it keeps the light colours.
  const isBlackMap = () => look.dark && (usingFallback || typeof styleUrl === "object");
  const colors = () => OWN_COLORS[isBlackMap() ? "dark" : "light"];
  const airspaceColor = () => [
    "match",
    ["get", "category"],
    ...Object.entries(colors().airspace).flat(),
    colors().airspace.other,
  ];
  // These are filled lightly; all others only get a band along the inside of the border,
  // so that large airspaces do not cover the map.
  const FILLED_AIRSPACE = ["ctr", "restricted", "danger", "prohibited"];
  // Drawn with a dashed border: zones with rules (transponder, radio) and class E.
  const DASHED_AIRSPACE = ["tmz", "rmz", "classE"];
  const SOLID_AIRSPACE = FILLED_AIRSPACE.concat(["controlled", "gliderSector", "reserved", "other"]);
  // The airspace layers and the categories each one draws (null: all).
  const AIRSPACE_LAYERS = {
    "airspace-fill": FILLED_AIRSPACE,
    "airspace-band": null,
    "airspace-line": SOLID_AIRSPACE,
    "airspace-line-dashed": DASHED_AIRSPACE,
    "airspace-label": null,
  };

  let look = { dark: false, relief: false };
  let map = null;
  let marker = null;
  let styleUrl = null;
  let usingFallback = false;
  let follow = true;
  let coordinates = []; // [longitude, latitude] pairs of the track
  let savedCoordinates = []; // the same for a recorded flight being viewed
  let airspaceData = { type: "FeatureCollection", features: [] };
  let hiddenAirspace = ["other"]; // categories not shown
  let warned = []; // { index, level } of the airspaces that warn
  let position = null; // { longitude, latitude, course }
  let onFollowChange = () => {};
  let onMessage = () => {};
  let onTap = () => {};
  let tapMarker = null;

  /** Map symbols by name, drawn on a canvas; `null` for unknown names. */
  function drawSymbol(name) {
    const size = 32; // drawn at pixel ratio 2: 16 px on the map
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const c = canvas.getContext("2d");
    c.lineJoin = "round";
    c.lineWidth = 3;
    c.strokeStyle = "#fff";
    if (name === "peak") {
      // Black triangle with a white edge.
      c.beginPath();
      c.moveTo(16, 4);
      c.lineTo(28, 26);
      c.lineTo(4, 26);
      c.closePath();
      c.stroke();
      c.fillStyle = "#2b2216";
      c.fill();
    } else if (name === "saddle") {
      // A small "pass" mark: two arcs facing each other, dark on a white edge.
      for (const color of ["#fff", "#3d3020"]) {
        c.strokeStyle = color;
        c.lineWidth = color === "#fff" ? 6 : 3;
        c.beginPath();
        c.arc(16, 2, 12, 0.35 * Math.PI, 0.65 * Math.PI);
        c.stroke();
        c.beginPath();
        c.arc(16, 30, 12, 1.35 * Math.PI, 1.65 * Math.PI);
        c.stroke();
      }
    } else if (name === "aerodrome") {
      // Blue circle with a runway bar.
      c.beginPath();
      c.arc(16, 16, 11, 0, 2 * Math.PI);
      c.fillStyle = "#1a3f8f";
      c.fill();
      c.stroke();
      c.fillStyle = "#fff";
      c.fillRect(7, 14, 18, 4);
    } else if (name === "fly-takeoff" || name === "fly-training") {
      // Takeoff: green triangle with a white edge (training hill: lighter green).
      c.beginPath();
      c.moveTo(16, 3);
      c.lineTo(29, 27);
      c.lineTo(3, 27);
      c.closePath();
      c.stroke();
      c.fillStyle = name === "fly-takeoff" ? "#1f8a24" : "#6cbf4f";
      c.fill();
    } else if (name === "fly-landing" || name === "fly-toplanding") {
      // Landing: yellow circle with "L" (top landing: "T").
      c.beginPath();
      c.arc(16, 16, 12, 0, 2 * Math.PI);
      c.fillStyle = "#f2c200";
      c.fill();
      c.strokeStyle = "#6b4e00";
      c.lineWidth = 2;
      c.stroke();
      c.fillStyle = "#3d2c00";
      c.font = "bold 17px sans-serif";
      c.textAlign = "center";
      c.textBaseline = "middle";
      c.fillText(name === "fly-landing" ? "L" : "T", 16, 17);
    } else if (name === "fly-site") {
      // Flying site of unknown kind: grey circle with a white edge.
      c.beginPath();
      c.arc(16, 16, 10, 0, 2 * Math.PI);
      c.stroke();
      c.fillStyle = "#6e6e6e";
      c.fill();
    } else if (name === "fly-obstacle") {
      // Aviation obstacle symbol: an inverted V with a dot at its foot, dark on a white edge.
      for (const color of ["#fff", "#1a1a1a"]) {
        c.strokeStyle = color;
        c.fillStyle = color;
        c.lineWidth = color === "#fff" ? 6 : 3;
        c.beginPath();
        c.moveTo(6, 28);
        c.lineTo(16, 4);
        c.lineTo(26, 28);
        c.stroke();
        c.beginPath();
        c.arc(16, 24, color === "#fff" ? 4.5 : 3, 0, 2 * Math.PI);
        c.fill();
      }
    } else {
      return null;
    }
    return c.getImageData(0, 0, size, size);
  }

  function markerElement() {
    const element = document.createElement("div");
    element.className = "position-marker";
    // Arrow pointing up; rotated to the course over ground.
    element.innerHTML =
      '<svg viewBox="0 0 26 26" width="26" height="26">' +
      '<path d="M13 1 L23 24 L13 19 L3 24 Z" style="fill:var(--ink);stroke:var(--paper)" stroke-width="2" stroke-linejoin="round"/>' +
      "</svg>";
    return element;
  }

  // Sources and layers belong to the style, so they must be added again after every
  // style change.
  function addOwnLayers() {
    // Airspaces first: the track goes on top of them.
    addAirspaceLayers();
    addTrackLayers();
    applyRelief();
  }

  /** Shows or hides the relief of a dark offline style; the light one keeps its own. */
  function applyRelief() {
    if (!map || !look.dark) return;
    for (const layer of RELIEF_LAYERS) {
      if (map.getLayer(layer)) {
        map.setLayoutProperty(layer, "visibility", look.relief ? "visible" : "none");
      }
    }
  }

  /** Recolours what the map draws itself, for a style that stays (an online one). */
  function applyOwnColors() {
    if (!map || !map.getStyle()) return;
    const paint = (layer, property, value) => {
      if (map.getLayer(layer)) map.setPaintProperty(layer, property, value);
    };
    for (const layer of ["airspace-band", "airspace-line", "airspace-line-dashed"]) {
      paint(layer, "line-color", airspaceColor());
    }
    paint("airspace-fill", "fill-color", airspaceColor());
    paint("airspace-warning", "line-color", warningColor());
    paint("airspace-label", "text-color", airspaceColor());
    paint("airspace-label", "text-halo-color", colors().halo);
    paint(SAVED_SOURCE + "-casing", "line-color", colors().casing);
    paint(SAVED_SOURCE + "-line", "line-color", colors().saved);
    paint(TRACK_SOURCE + "-casing", "line-color", colors().casing);
    paint(TRACK_SOURCE + "-line", "line-color", colors().track);
    if (usingFallback) paint("background", "background-color", colors().fallback);
  }

  /** Filter expression for the visible airspaces drawn by a layer. */
  function airspaceFilter(layer) {
    const categories = AIRSPACE_LAYERS[layer];
    const visible = ["!", ["in", ["get", "category"], ["literal", hiddenAirspace]]];
    if (!categories) return visible;
    return ["all", visible, ["in", ["get", "category"], ["literal", categories]]];
  }

  function warningFilter() {
    return ["in", ["get", "index"], ["literal", warned.map((w) => w.index)]];
  }

  function warningColor() {
    const alarms = warned.filter((w) => w.level === "alarm").map((w) => w.index);
    return ["case", ["in", ["get", "index"], ["literal", alarms]], colors().alarm, colors().near];
  }

  function addAirspaceLayers() {
    if (map.getSource(AIRSPACE_SOURCE)) return;
    map.addSource(AIRSPACE_SOURCE, {
      type: "geojson",
      data: airspaceData,
      attribution: "Lufträume © openAIP (CC BY-NC 4.0)",
    });
    map.addLayer({
      id: "airspace-fill",
      type: "fill",
      source: AIRSPACE_SOURCE,
      filter: airspaceFilter("airspace-fill"),
      paint: { "fill-color": airspaceColor(), "fill-opacity": 0.1 },
    });
    // The rings come counterclockwise (Rust makes sure of it); on the screen, with y
    // pointing down, that is clockwise, so a positive offset lies inside.
    map.addLayer({
      id: "airspace-band",
      type: "line",
      source: AIRSPACE_SOURCE,
      filter: airspaceFilter("airspace-band"),
      layout: { "line-join": "round" },
      paint: {
        "line-color": airspaceColor(),
        "line-opacity": 0.25,
        "line-width": ["interpolate", ["linear"], ["zoom"], 7, 3, 12, 8],
        "line-offset": ["interpolate", ["linear"], ["zoom"], 7, 1.5, 12, 4],
      },
    });
    map.addLayer({
      id: "airspace-line",
      type: "line",
      source: AIRSPACE_SOURCE,
      filter: airspaceFilter("airspace-line"),
      layout: { "line-join": "round" },
      paint: { "line-color": airspaceColor(), "line-width": 1.4 },
    });
    map.addLayer({
      id: "airspace-line-dashed",
      type: "line",
      source: AIRSPACE_SOURCE,
      filter: airspaceFilter("airspace-line-dashed"),
      paint: { "line-color": airspaceColor(), "line-width": 1.4, "line-dasharray": [4, 2] },
    });
    // Airspaces that warn: a thick border, also for categories the map hides.
    map.addLayer({
      id: "airspace-warning",
      type: "line",
      source: AIRSPACE_SOURCE,
      filter: warningFilter(),
      layout: { "line-join": "round" },
      paint: { "line-color": warningColor(), "line-width": 5, "line-opacity": 0.85 },
    });
    // Labels need glyphs, which the fallback style does not have.
    if (!map.getStyle().glyphs) return;
    map.addLayer({
      id: "airspace-label",
      type: "symbol",
      source: AIRSPACE_SOURCE,
      minzoom: 9,
      filter: airspaceFilter("airspace-label"),
      layout: {
        "symbol-placement": "line",
        "symbol-spacing": 350,
        "text-field": ["get", "label"],
        "text-font": ["Noto Sans Regular"],
        "text-size": 11,
        "text-max-angle": 30,
        "text-keep-upright": true,
      },
      paint: {
        "text-color": airspaceColor(),
        "text-halo-color": colors().halo,
        "text-halo-width": 1.5,
      },
    });
  }

  function addTrackLayers() {
    if (map.getSource(TRACK_SOURCE)) return;
    // The recorded flight being viewed goes below the live track.
    addLine(SAVED_SOURCE, savedCoordinates, colors().saved);
    addLine(TRACK_SOURCE, coordinates, colors().track);
  }

  function addLine(source, points, color) {
    map.addSource(source, { type: "geojson", data: lineData(points) });
    // A casing in the colour of the ground sets the line apart from what lies below.
    map.addLayer({
      id: source + "-casing",
      type: "line",
      source,
      layout: { "line-join": "round", "line-cap": "round" },
      paint: { "line-color": colors().casing, "line-width": 7 },
    });
    map.addLayer({
      id: source + "-line",
      type: "line",
      source,
      layout: { "line-join": "round", "line-cap": "round" },
      paint: { "line-color": color, "line-width": 4 },
    });
  }

  function lineData(points) {
    return {
      type: "Feature",
      properties: {},
      geometry: { type: "LineString", coordinates: points },
    };
  }

  function refresh(source, points) {
    const mapSource = map && map.getSource(source);
    if (mapSource) mapSource.setData(lineData(points));
  }

  const refreshTrack = () => refresh(TRACK_SOURCE, coordinates);

  function fitSavedFlight() {
    if (!map || savedCoordinates.length === 0) return;
    const bounds = new maplibregl.LngLatBounds(savedCoordinates[0], savedCoordinates[0]);
    for (const point of savedCoordinates) bounds.extend(point);
    map.fitBounds(bounds, { padding: 70, maxZoom: 15, duration: 0 });
  }

  function setFollow(value) {
    follow = value;
    onFollowChange(follow);
    if (follow && position && map) {
      map.easeTo({ center: [position.longitude, position.latitude], duration: 300 });
    }
  }

  function create() {
    map = new maplibregl.Map({
      container: "map",
      style: styleUrl,
      center: position ? [position.longitude, position.latitude] : [11.0, 47.6],
      zoom: position ? 14 : 8,
      // The attribution must always be visible (terms of the tile providers).
      attributionControl: { compact: false },
      dragRotate: false,
      pitchWithRotate: false,
    });
    map.touchZoomRotate.disableRotation();
    map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");

    map.on("style.load", addOwnLayers);
    // The offline styles use a few symbols that are drawn here instead of being
    // loaded from a sprite file.
    map.on("styleimagemissing", (event) => {
      const image = drawSymbol(event.id);
      if (image && !map.hasImage(event.id)) map.addImage(event.id, image, { pixelRatio: 2 });
    });
    // A flight chosen before the map existed: show it once the map has its size.
    map.once("load", fitSavedFlight);
    // Moving the map by hand stops following; the "Folgen" button turns it back on.
    map.on("dragstart", () => setFollow(false));
    // A tap (not a drag) asks for the airspaces at that place.
    map.on("click", (event) => onTap(event.lngLat.lat, event.lngLat.lng));
    map.on("error", (event) => {
      // If the style itself cannot be loaded there is no map at all: fall back once.
      if (!map.isStyleLoaded() && !usingFallback && !map.getSource(TRACK_SOURCE)) {
        usingFallback = true;
        map.setStyle(emptyStyle(), { diff: false });
        onMessage("Karte nicht ladbar (keine Verbindung?). Track und Position werden trotzdem gezeigt.");
      } else {
        console.warn("map error", event.error);
      }
    });

    marker = new maplibregl.Marker({ element: markerElement(), rotationAlignment: "map" });
    if (position) showPosition();
  }

  function showPosition() {
    marker.setLngLat([position.longitude, position.latitude]);
    marker.setRotation(position.course ?? 0);
    marker.addTo(map);
    if (follow) map.jumpTo({ center: [position.longitude, position.latitude] });
  }

  return {
    configure(options) {
      onFollowChange = options.onFollowChange;
      onMessage = options.onMessage;
      onTap = options.onTap ?? (() => {});
    },

    /** Marks the tapped place (or removes the mark with null). */
    markTap(latitude, longitude) {
      if (latitude === null) {
        tapMarker?.remove();
        return;
      }
      if (!tapMarker) {
        const element = document.createElement("div");
        element.className = "tap-marker";
        tapMarker = new maplibregl.Marker({ element });
      }
      tapMarker.setLngLat([longitude, latitude]).addTo(map);
    },

    airspaceColor: (category) => themeColors().airspace[category] ?? themeColors().airspace.other,

    /** Light or dark and the relief: `{ dark, relief }`. Changes nothing about the flight. */
    setLook(next) {
      look = { dark: next.dark, relief: next.relief };
      applyOwnColors();
      applyRelief();
    },

    /** What the map shows of the look; for the smoke test. */
    lookState() {
      const present = map ? RELIEF_LAYERS.filter((layer) => map.getLayer(layer)) : [];
      return {
        fallbackBackground: themeColors().fallback,
        relief: present.length
          ? present.some((layer) => map.getLayoutProperty(layer, "visibility") !== "none")
          : null,
        // Whether the contour lines show; `null` if the style has none.
        contours: map && map.getLayer("contour-major")
          ? map.getLayoutProperty("contour-major", "visibility") !== "none"
          : null,
        blackMap: isBlackMap(),
        trackColor: map && map.getLayer(TRACK_SOURCE + "-line")
          ? map.getPaintProperty(TRACK_SOURCE + "-line", "line-color")
          : null,
        ownLayers: Boolean(
          map && map.getLayer("airspace-line") && map.getLayer(TRACK_SOURCE + "-line"),
        ),
      };
    },

    /** Sets the style (URL or style object); reloads the map if it already exists. */
    setStyle(url) {
      styleUrl = url;
      usingFallback = false;
      onMessage("");
      // Without `diff: false` the map would only compare the two styles and remove the
      // layers it does not find in the new one, without the event that adds them again.
      if (map) map.setStyle(url, { diff: false });
    },

    /** Call when the map page becomes visible: WebGL needs a visible container. */
    show() {
      if (!map) {
        create();
      } else {
        map.resize();
        if (savedCoordinates.length > 0 && !follow) fitSavedFlight();
        else if (follow && position) map.jumpTo({ center: [position.longitude, position.latitude] });
      }
    },

    /** Latest position; `course` in degrees or null. */
    setPosition(latitude, longitude, course) {
      const first = position === null;
      position = { latitude, longitude, course };
      if (!map) return;
      if (first && follow) map.jumpTo({ center: [longitude, latitude], zoom: 14 });
      showPosition();
    },

    /** Appends points ({ latitudeDeg, longitudeDeg }) to the track. */
    addTrackPoints(points) {
      for (const point of points) coordinates.push([point.longitudeDeg, point.latitudeDeg]);
      refreshTrack();
    },

    clearTrack() {
      coordinates = [];
      refreshTrack();
    },

    /** Shows a recorded flight (points as from Rust) and zooms to it. */
    showSavedFlight(points) {
      savedCoordinates = points.map((point) => [point.longitudeDeg, point.latitudeDeg]);
      refresh(SAVED_SOURCE, savedCoordinates);
      // Otherwise the map would jump straight back to the own position.
      setFollow(false);
      fitSavedFlight();
    },

    hideSavedFlight() {
      savedCoordinates = [];
      refresh(SAVED_SOURCE, savedCoordinates);
    },

    /** Shows the airspaces (a GeoJSON feature collection). */
    setAirspaces(geojson) {
      airspaceData = geojson;
      const source = map && map.getSource(AIRSPACE_SOURCE);
      if (source) source.setData(airspaceData);
    },

    /** Hides the airspaces of these categories (names as in airspace.rs). */
    setAirspaceHidden(categories) {
      hiddenAirspace = categories;
      if (!map) return;
      for (const layer of Object.keys(AIRSPACE_LAYERS)) {
        if (map.getLayer(layer)) map.setFilter(layer, airspaceFilter(layer));
      }
    },

    /** Highlights the airspaces that warn: a list of { index, level }. */
    setWarnings(warnings) {
      const next = warnings.map((w) => ({ index: w.index, level: w.level }));
      if (JSON.stringify(next) === JSON.stringify(warned)) return;
      warned = next;
      if (!map || !map.getLayer("airspace-warning")) return;
      map.setFilter("airspace-warning", warningFilter());
      map.setPaintProperty("airspace-warning", "line-color", warningColor());
    },

    trackLength: () => coordinates.length,
    warnedCount: () => warned.length,
    airspaceCount: () => airspaceData.features.length,
    toggleFollow: () => setFollow(!follow),
  };
})();
