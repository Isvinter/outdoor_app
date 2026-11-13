#!/usr/bin/env python3
"""Generates the MapLibre styles in styles/ from one description.

    python3 scripts/make-style.py

The styles use placeholders that the app fills in when loading them:
__TILES__ (base URL of the tile protocol), __FONTS__ (base URL of the glyph
files) and __CONTOURS__ (tile URL of the contour line generator).
"""

import json

NAME = ["coalesce", ["get", "name:de"], ["get", "name"]]
REG, BOLD, ITAL = ["Noto Sans Regular"], ["Noto Sans Bold"], ["Noto Sans Italic"]

def zoom(*stops, base=1.4):
    return ["interpolate", ["exponential", base], ["zoom"], *stops]

def road(cls, color, casing, w, minzoom, cap="round"):
    """Casing and fill layer for one road class; w = (zoom, width) stops of the fill."""
    flt = ["all", ["==", ["geometry-type"], "LineString"], ["in", ["get", "class"], ["literal", cls]],
           ["!=", ["get", "brunnel"], "tunnel"]]
    fill = [v for z, x in w for v in (z, x)]
    case = [v for z, x in w for v in (z, x + 1.6)]
    ident = cls[0]
    return (
        {"id": f"road-{ident}-casing", "type": "line", "source": "basemap", "source-layer": "transportation",
         "minzoom": minzoom, "filter": flt, "layout": {"line-join": "round", "line-cap": "butt"},
         "paint": {"line-color": casing, "line-width": zoom(*case)}},
        {"id": f"road-{ident}", "type": "line", "source": "basemap", "source-layer": "transportation",
         "minzoom": minzoom, "filter": flt, "layout": {"line-join": "round", "line-cap": cap},
         "paint": {"line-color": color, "line-width": zoom(*fill)}},
    )

roads = [
    road(["service"], "#ffffff", "#c9c2b4", [(13, 0.6), (16, 3), (18, 8)], 13),
    road(["minor"], "#ffffff", "#bdb5a5", [(11, 0.5), (14, 2.2), (16, 6), (18, 14)], 11),
    road(["tertiary"], "#fffbe0", "#b5a989", [(9, 0.6), (12, 2), (14, 3.5), (16, 8), (18, 16)], 9),
    road(["secondary"], "#fbee9c", "#b59f58", [(8, 0.6), (12, 2.4), (14, 4), (16, 9), (18, 18)], 8),
    road(["primary"], "#f8d877", "#b08d3a", [(7, 0.7), (12, 2.8), (14, 4.5), (16, 10), (18, 20)], 7),
    road(["trunk", "motorway"], "#f0a65a", "#a8662a", [(5, 0.8), (10, 2.4), (14, 5), (16, 11), (18, 22)], 5),
]
casings = [c for c, _ in roads]
fills = [f for _, f in roads]

PATHS = ["path", "footway", "bridleway", "steps", "via_ferrata"]
ALPINE = ["alpine_hiking", "demanding_alpine_hiking", "difficult_alpine_hiking"]
MOUNTAIN = ["mountain_hiking", "demanding_mountain_hiking"]

def trail(ident, flt, paint, minzoom=12, layout=None):
    return {"id": ident, "type": "line", "source": "outdoor", "source-layer": "trail", "minzoom": minzoom,
            "filter": flt, "layout": layout or {"line-join": "round", "line-cap": "butt"}, "paint": paint}

def route(ident, kind, color, minzoom=7):
    return {"id": ident, "type": "line", "source": "outdoor", "source-layer": "route", "minzoom": minzoom,
            "filter": ["==", ["get", "kind"], kind], "layout": {"line-join": "round", "line-cap": "round"},
            "paint": {"line-color": color, "line-opacity": 0.32,
                      "line-width": zoom(8, 1.5, 12, 3.5, 15, 4.5, 18, 7, base=1.2)}}

def poi(ident, kinds, icon, minzoom, text, color="#3d3020", font=REG, size=11):
    return {"id": ident, "type": "symbol", "source": "outdoor", "source-layer": "poi", "minzoom": minzoom,
            "filter": ["in", ["get", "kind"], ["literal", kinds]],
            "layout": {"icon-image": icon, "icon-size": zoom(10, 0.7, 15, 1, base=1), "text-field": text,
                       "text-font": font, "text-size": size, "text-anchor": "top", "text-offset": [0, 0.7],
                       "text-optional": True, "text-max-width": 8,
                       "symbol-sort-key": ["-", 0, ["coalesce", ["get", "ele"], 0]]},
            "paint": {"text-color": color, "text-halo-color": "rgba(255,255,255,0.85)", "text-halo-width": 1.4}}

def place(ident, classes, minzoom, size, font=REG, color="#222", maxzoom=None, transform=None):
    layer = {"id": ident, "type": "symbol", "source": "basemap", "source-layer": "place", "minzoom": minzoom,
             "filter": ["in", ["get", "class"], ["literal", classes]],
             "layout": {"text-field": NAME, "text-font": font, "text-size": size, "text-max-width": 8,
                        "symbol-sort-key": ["coalesce", ["get", "rank"], 99]},
             "paint": {"text-color": color, "text-halo-color": "rgba(255,255,255,0.9)", "text-halo-width": 1.6}}
    if maxzoom: layer["maxzoom"] = maxzoom
    if transform: layer["layout"]["text-transform"] = transform
    return layer

ELE_NAME = ["case", ["has", "ele"],
            ["format", ["coalesce", ["get", "name"], ""], {}, "\n", {},
             ["concat", ["to-string", ["get", "ele"]], " m"], {"font-scale": 0.85}],
            ["coalesce", ["get", "name"], ""]]

landcover = lambda ident, classes, color, opacity=1.0: {
    "id": f"landcover-{ident}", "type": "fill", "source": "basemap", "source-layer": "landcover",
    "filter": ["in", ["get", "class"], ["literal", classes]],
    "paint": {"fill-color": color, "fill-opacity": opacity, "fill-antialias": False}}

layers = [
    {"id": "background", "type": "background", "paint": {"background-color": "#f5f2e8"}},
    landcover("farmland", ["farmland"], "#f1efd9"),
    landcover("grass", ["grass"], "#dcebc2"),
    landcover("wood", ["wood"], "#bcdaa4"),
    landcover("wetland", ["wetland"], "#cfe3d2"),
    landcover("sand", ["sand"], "#efe5c0"),
    landcover("rock", ["rock"], "#dedad3"),
    landcover("ice", ["ice"], "#eaf5fa"),
    {"id": "landuse-residential", "type": "fill", "source": "basemap", "source-layer": "landuse",
     "filter": ["in", ["get", "class"], ["literal", ["residential", "commercial", "industrial", "retail"]]],
     "paint": {"fill-color": "#e9e3d8", "fill-opacity": zoom(8, 0.9, 14, 0.6, base=1)}},
    {"id": "hillshade", "type": "hillshade", "source": "dem",
     "paint": {"hillshade-exaggeration": zoom(6, 0.5, 13, 0.4, 16, 0.25, base=1),
               "hillshade-shadow-color": "#4a4234", "hillshade-highlight-color": "#ffffff",
               "hillshade-accent-color": "#6b5f4a"}},
    {"id": "contour-minor", "type": "line", "source": "contours", "source-layer": "contours", "minzoom": 11,
     "filter": ["!=", ["get", "level"], 1],
     "paint": {"line-color": "#a5713a", "line-opacity": 0.45, "line-width": 0.6}},
    {"id": "contour-major", "type": "line", "source": "contours", "source-layer": "contours", "minzoom": 11,
     "filter": ["==", ["get", "level"], 1],
     "paint": {"line-color": "#a5713a", "line-opacity": 0.7, "line-width": 1.1}},
    {"id": "waterway", "type": "line", "source": "basemap", "source-layer": "waterway",
     "filter": ["!=", ["get", "brunnel"], "tunnel"], "layout": {"line-cap": "round"},
     "paint": {"line-color": "#7fb3de",
               "line-width": zoom(*[v for z, w in ((8, 0.5), (13, 1.2), (17, 4))
                                    for v in (z, ["*", w, ["match", ["get", "class"], "river", 1.8, "canal", 1.4, 0.8]])],
                                  base=1.3)}},
    {"id": "water", "type": "fill", "source": "basemap", "source-layer": "water",
     "filter": ["!=", ["get", "brunnel"], "tunnel"], "paint": {"fill-color": "#a9cfec"}},
    {"id": "building", "type": "fill", "source": "basemap", "source-layer": "building", "minzoom": 13,
     "paint": {"fill-color": "#cfc6b8", "fill-opacity": zoom(13, 0.5, 15, 0.9, base=1)}},
    {"id": "road-tunnel", "type": "line", "source": "basemap", "source-layer": "transportation", "minzoom": 9,
     "filter": ["all", ["==", ["get", "brunnel"], "tunnel"],
                ["in", ["get", "class"], ["literal", ["motorway", "trunk", "primary", "secondary", "tertiary", "minor"]]]],
     "paint": {"line-color": "#b9b1a2", "line-dasharray": [3, 2], "line-width": zoom(9, 0.8, 14, 2.5, 18, 10)}},

    # --- trails ---------------------------------------------------------------
    trail("trail-mtb", ["all", ["has", "mtb_scale"], ["!=", ["get", "kind"], "track"]],
          {"line-color": ["step", ["get", "mtb_scale"], "#2e9d4f", 1, "#2b7bd6", 2, "#d9662b", 3, "#2a2a2a"],
           "line-opacity": 0.38, "line-width": zoom(12, 3, 15, 7, 18, 12, base=1.2)},
          layout={"line-join": "round", "line-cap": "round"}),
    trail("trail-track-casing", ["==", ["get", "kind"], "track"],
          {"line-color": "#8d7350", "line-width": zoom(11, 0.9, 14, 2.6, 16, 5, 18, 10)}, minzoom=11),
    trail("trail-track", ["==", ["get", "kind"], "track"],
          {"line-color": ["match", ["get", "tracktype"], ["grade1", "grade2"], "#ffffff", "#f3ead8"],
           "line-width": zoom(11, 0.3, 14, 1.4, 16, 3.4, 18, 8)}, minzoom=12),
    trail("trail-cycleway", ["==", ["get", "kind"], "cycleway"],
          {"line-color": "#3d6fb3", "line-width": zoom(11, 0.7, 14, 1.5, 18, 4)}, minzoom=11),
    trail("trail-path", ["all", ["in", ["get", "kind"], ["literal", PATHS]],
                         ["!", ["in", ["coalesce", ["get", "sac_scale"], ""], ["literal", MOUNTAIN + ALPINE]]]],
          {"line-color": "#5b4a36", "line-dasharray": [4, 1.5], "line-width": zoom(12, 0.7, 15, 1.4, 18, 3)}),
    trail("trail-path-mountain", ["in", ["coalesce", ["get", "sac_scale"], ""], ["literal", MOUNTAIN]],
          {"line-color": "#c4261d", "line-dasharray": [4, 1.5], "line-width": zoom(12, 0.9, 15, 1.6, 18, 3.2)}),
    trail("trail-path-alpine", ["in", ["coalesce", ["get", "sac_scale"], ""], ["literal", ALPINE]],
          {"line-color": "#1f4fa3", "line-dasharray": [1.2, 1.6], "line-width": zoom(12, 1, 15, 1.8, 18, 3.4)}),

    *casings,
    *fills,
    {"id": "rail", "type": "line", "source": "basemap", "source-layer": "transportation", "minzoom": 8,
     "filter": ["all", ["==", ["get", "class"], "rail"], ["!=", ["get", "brunnel"], "tunnel"]],
     "paint": {"line-color": "#5f5f5f", "line-width": zoom(8, 0.6, 14, 1.6, 18, 4)}},
    {"id": "rail-ties", "type": "line", "source": "basemap", "source-layer": "transportation", "minzoom": 12,
     "filter": ["all", ["==", ["get", "class"], "rail"], ["!=", ["get", "brunnel"], "tunnel"]],
     "paint": {"line-color": "#ffffff", "line-dasharray": [4, 4], "line-width": zoom(12, 0.5, 14, 0.9, 18, 2.4)}},
    {"id": "aerialway", "type": "line", "source": "basemap", "source-layer": "transportation", "minzoom": 11,
     "filter": ["==", ["get", "class"], "aerialway"],
     "paint": {"line-color": "#333333", "line-width": zoom(11, 0.6, 15, 1.4, base=1)}},

    # --- signposted routes -----------------------------------------------------
    route("route-bicycle", "bicycle", "#0a86b8"),
    route("route-mtb", "mtb", "#e07b00", 9),
    route("route-hiking", "hiking", "#d32f2f", 9),

    {"id": "boundary-state", "type": "line", "source": "basemap", "source-layer": "boundary", "minzoom": 5,
     "filter": ["all", ["==", ["get", "admin_level"], 4], ["!=", ["get", "maritime"], 1]],
     "paint": {"line-color": "#8a6aa0", "line-dasharray": [5, 3], "line-opacity": 0.6, "line-width": zoom(5, 0.6, 12, 1.4, base=1)}},
    {"id": "boundary-country", "type": "line", "source": "basemap", "source-layer": "boundary",
     "filter": ["all", ["<=", ["get", "admin_level"], 2], ["!=", ["get", "maritime"], 1]],
     "paint": {"line-color": "#7a5594", "line-opacity": 0.7, "line-width": zoom(3, 0.8, 12, 2.4, base=1)}},

    # --- labels ---------------------------------------------------------------
    {"id": "contour-label", "type": "symbol", "source": "contours", "source-layer": "contours", "minzoom": 13,
     "filter": ["==", ["get", "level"], 1],
     "layout": {"symbol-placement": "line", "symbol-spacing": 350, "text-field": ["to-string", ["get", "ele"]],
                "text-font": REG, "text-size": 10, "text-max-angle": 25},
     "paint": {"text-color": "#8a5a26", "text-halo-color": "rgba(245,242,232,0.9)", "text-halo-width": 1.2}},
    {"id": "waterway-label", "type": "symbol", "source": "basemap", "source-layer": "waterway", "minzoom": 13,
     "filter": ["has", "name"],
     "layout": {"symbol-placement": "line", "symbol-spacing": 400, "text-field": NAME, "text-font": ITAL, "text-size": 11},
     "paint": {"text-color": "#2f6ea5", "text-halo-color": "rgba(255,255,255,0.8)", "text-halo-width": 1.2}},
    {"id": "water-label", "type": "symbol", "source": "basemap", "source-layer": "water_name", "minzoom": 9,
     "filter": ["==", ["geometry-type"], "Point"],
     "layout": {"text-field": NAME, "text-font": ITAL, "text-size": zoom(9, 10, 14, 13, base=1), "text-max-width": 6},
     "paint": {"text-color": "#2f6ea5", "text-halo-color": "rgba(255,255,255,0.8)", "text-halo-width": 1.2}},
    {"id": "trail-label", "type": "symbol", "source": "outdoor", "source-layer": "trail", "minzoom": 15,
     "filter": ["has", "name"],
     "layout": {"symbol-placement": "line", "symbol-spacing": 400, "text-field": ["get", "name"], "text-font": REG,
                "text-size": 10, "text-offset": [0, 0.8]},
     "paint": {"text-color": "#4a3b28", "text-halo-color": "rgba(255,255,255,0.85)", "text-halo-width": 1.2}},
    {"id": "route-label", "type": "symbol", "source": "outdoor", "source-layer": "route", "minzoom": 12,
     "filter": ["has", "ref"],
     "layout": {"symbol-placement": "line", "symbol-spacing": 500, "text-field": ["get", "ref"], "text-font": BOLD,
                "text-size": 10, "text-rotation-alignment": "viewport"},
     "paint": {"text-color": ["match", ["get", "kind"], "hiking", "#a81f1f", "mtb", "#a85c00", "#06658c"],
               "text-halo-color": "#ffffff", "text-halo-width": 1.6}},
    {"id": "road-label", "type": "symbol", "source": "basemap", "source-layer": "transportation_name", "minzoom": 13,
     "filter": ["in", ["get", "class"], ["literal", ["motorway", "trunk", "primary", "secondary", "tertiary", "minor"]]],
     "layout": {"symbol-placement": "line", "symbol-spacing": 400, "text-field": NAME, "text-font": REG,
                "text-size": zoom(13, 10, 17, 12, base=1)},
     "paint": {"text-color": "#444444", "text-halo-color": "rgba(255,255,255,0.9)", "text-halo-width": 1.4}},
    poi("poi-water", ["spring"], "spring", 14, ["coalesce", ["get", "name"], ""], color="#2f6ea5", size=10),
    poi("poi-view", ["viewpoint"], "viewpoint", 14, ["coalesce", ["get", "name"], ""], size=10),
    poi("poi-shelter", ["shelter", "wilderness_hut"], "shelter", 13, ["coalesce", ["get", "name"], ""], size=10),
    poi("poi-saddle", ["saddle"], "saddle", 12, ELE_NAME, size=10),
    poi("poi-hut", ["alpine_hut"], "hut", 10, ELE_NAME, color="#7a1f1f", font=BOLD),
    poi("poi-peak", ["peak"], "peak", 10, ELE_NAME, font=BOLD),
    place("place-hamlet", ["hamlet", "isolated_dwelling", "neighbourhood", "quarter"], 13, 11, color="#444"),
    place("place-suburb", ["suburb"], 12, 11, color="#555", transform="uppercase"),
    place("place-village", ["village"], 10, zoom(10, 11, 15, 14, base=1)),
    place("place-town", ["town"], 7, zoom(7, 11, 14, 16, base=1), font=BOLD),
    place("place-city", ["city"], 4, zoom(4, 11, 12, 18, base=1), font=BOLD),
]

SOURCES = {
    "basemap": {"type": "vector", "tiles": ["__TILES__basemap/{z}/{x}/{y}"], "minzoom": 0, "maxzoom": 14},
    "outdoor": {"type": "vector", "tiles": ["__TILES__outdoor/{z}/{x}/{y}"], "minzoom": 7, "maxzoom": 14},
    "dem": {"type": "raster-dem", "tiles": ["__TILES__dem/{z}/{x}/{y}"], "encoding": "terrarium",
            "tileSize": 512, "minzoom": 0, "maxzoom": 13},
    # Replaced at run time by the contour line generator working on the "dem" tiles.
    "contours": {"type": "vector", "tiles": ["__CONTOURS__"], "maxzoom": 15},
}

# --- winter: snow colours, slope angle classes, pistes and ski routes instead of summer routes ---

WINTER_PAINT = {
    "background": {"background-color": "#fbfcfd"},
    "landcover-farmland": {"fill-color": "#f6f8f6"},
    "landcover-grass": {"fill-color": "#f3f6f2"},
    "landcover-wood": {"fill-color": "#dde8da"},
    "landcover-wetland": {"fill-color": "#eaf1ee"},
    "landcover-sand": {"fill-color": "#f4f2ea"},
    "landcover-rock": {"fill-color": "#e7e7e9"},
    "landcover-ice": {"fill-color": "#e3f0f8"},
    "landuse-residential": {"fill-color": "#ece9e4"},
    "contour-minor": {"line-color": "#7c8794"},
    "contour-major": {"line-color": "#7c8794"},
    "contour-label": {"text-color": "#5d6874", "text-halo-color": "rgba(251,252,253,0.9)"},
    "trail-path": {"line-color": "#8b8b8b"},
    "trail-path-mountain": {"line-color": "#8b8b8b"},
    "trail-path-alpine": {"line-color": "#8b8b8b"},
    "trail-track-casing": {"line-color": "#a39f98"},
    "trail-cycleway": {"line-color": "#9aa7b8"},
}
WINTER_DROPPED = {"trail-mtb", "route-bicycle", "route-mtb", "route-hiking", "route-label", "poi-water"}

def piste(ident, flt, paint, minzoom=11):
    return {"id": ident, "type": "line", "source": "outdoor", "source-layer": "piste", "minzoom": minzoom,
            "filter": flt, "layout": {"line-join": "round", "line-cap": "round"}, "paint": paint}

SLOPE = {"id": "slope", "type": "raster", "source": "slope", "minzoom": 9,
         "paint": {"raster-opacity": 0.6, "raster-resampling": "nearest", "raster-fade-duration": 0}}
PISTES = [
    piste("piste-downhill", ["==", ["get", "kind"], "downhill"],
          {"line-color": ["match", ["get", "difficulty"], "novice", "#3aa655", "easy", "#2b7bd6",
                          "intermediate", "#d62b2b", ["advanced", "expert", "freeride"], "#222222", "#6b6b6b"],
           "line-opacity": 0.55, "line-width": zoom(11, 1.5, 14, 5, 17, 12, base=1.3)}),
    piste("piste-nordic", ["==", ["get", "kind"], "nordic"],
          {"line-color": "#0f8f8f", "line-dasharray": [3, 1.5], "line-width": zoom(11, 1, 15, 2.2, base=1)}),
    piste("piste-sled", ["in", ["get", "kind"], ["literal", ["sled", "hike", "sleigh"]]],
          {"line-color": "#8a5fb0", "line-dasharray": [1, 1.5], "line-width": zoom(11, 1, 15, 2, base=1)}, 12),
    piste("piste-skitour", ["==", ["get", "kind"], "skitour"],
          {"line-color": "#c2187a", "line-dasharray": [3, 1.2], "line-width": zoom(10, 1.2, 15, 2.6, base=1)}, 10),
]
PISTE_LABEL = {
    "id": "piste-label", "type": "symbol", "source": "outdoor", "source-layer": "piste", "minzoom": 13,
    "filter": ["has", "name"],
    "layout": {"symbol-placement": "line", "symbol-spacing": 400, "text-field": ["get", "name"],
               "text-font": REG, "text-size": 10, "text-offset": [0, 0.8]},
    "paint": {"text-color": ["match", ["get", "kind"], "skitour", "#96105d", "#3c4650"],
              "text-halo-color": "rgba(255,255,255,0.9)", "text-halo-width": 1.3}}

def winter(summer):
    out = []
    for layer in summer:
        if layer["id"] in WINTER_DROPPED:
            continue
        layer = json.loads(json.dumps(layer))
        layer["paint"].update(WINTER_PAINT.get(layer["id"], {}))
        out.append(layer)
        if layer["id"] == "hillshade":
            out.append(SLOPE)
        if layer["id"] == "aerialway":
            out.extend(PISTES)
        if layer["id"] == "boundary-country":
            out.append(PISTE_LABEL)
    return out

def write(file, name, layers, sources):
    ids = [l["id"] for l in layers]
    assert len(ids) == len(set(ids)), "duplicate layer id"
    assert all(l.get("source", "basemap") in sources for l in layers), "layer without source"
    style = {
        "version": 8,
        "name": name,
        "metadata": {
            "about": "Generated by scripts/make-style.py; edit the script, not this file.",
            "placeholders": {"__TILES__": "base URL of the tile protocol", "__FONTS__": "base URL of the glyph files",
                             "__CONTOURS__": "tile URL of the contour line generator"},
        },
        "glyphs": "__FONTS__{fontstack}/{range}.pbf",
        "sources": sources,
        "layers": layers,
    }
    with open(file, "w", encoding="utf-8") as f:
        json.dump(style, f, ensure_ascii=False, indent=1)
        f.write("\n")
    print(f"{file}: {len(layers)} layers")

import os
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
write("styles/topo-summer.json", "Topo Sommer", layers, SOURCES)
write("styles/topo-winter.json", "Topo Winter", winter(layers),
      {**SOURCES, "slope": {"type": "raster", "tiles": ["__TILES__slope/{z}/{x}/{y}"], "tileSize": 512,
                            "minzoom": 9, "maxzoom": 13}})
