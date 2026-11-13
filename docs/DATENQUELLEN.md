# Datenquellen, Lizenzen und Attribution

Was in einem Regionspaket steckt, woher es kommt und wie es genannt werden muss. Die für
eine Region zutreffenden Zeilen stehen in `regions/<id>.json` (`attribution`), wandern beim
Bau ins Manifest und werden von der App unten rechts in der Karte und unter
**Über / Datenquellen** angezeigt.

## Kartendaten

| Datei im Paket | Inhalt | Quelle | Lizenz | Nennung |
|---|---|---|---|---|
| `basemap.pmtiles` | Basiskarte (Vektor, Zoom 0–14) | OpenStreetMap über Geofabrik-Auszüge, gebaut mit Planetiler im OpenMapTiles-Schema | Daten ODbL 1.0; Schema CC BY 4.0 | „© OpenStreetMap-Mitwirkende“, „© OpenMapTiles“ |
| `outdoor.pmtiles` | Wege mit `sac_scale`, `mtb:scale`, Belag; Wander-/Rad-/MTB-Routen; Pisten, Loipen, Skirouten; Gipfel, Hütten, Quellen | OpenStreetMap, eigenes Planetiler-Profil (`scripts/tiles/OutdoorOverlay.java`) | ODbL 1.0 | wie oben |
| `dem.pmtiles` | Höhenmodell (Terrarium-Kodierung) | Mapterhorn, ausgeschnitten mit `pmtiles extract` | je Quellgebiet, siehe unten | siehe unten |
| `slope.pmtiles` | Hangneigungsklassen | aus `dem.pmtiles` berechnet (`tools/slope-tiles`) | wie Höhenmodell | wie Höhenmodell |
| `routing.tar` | Routinggraph (Valhalla) | OpenStreetMap; Steigungen aus den Höhendaten, die Valhallas Werkzeug `valhalla_build_elevation` lädt (SRTM-HGT-Format, etwa 30 m) | ODbL 1.0 | wie Basiskarte |

Küstenlinien und Übersichtsdaten der Basiskarte: OpenStreetMap-Wasserpolygone
(osmdata.openstreetmap.de, ODbL) und Natural Earth (gemeinfrei).

### Höhenmodell nach Gebiet

Mapterhorn fasst amtliche Geländemodelle zusammen; die Lizenz richtet sich nach der Quelle.
Für Bayern und Tirol:

| Gebiet | Quelle | Auflösung | Lizenz | Nennung |
|---|---|---|---|---|
| Bayern | DGM1 | 1 m | CC BY 4.0 | „Bayerische Vermessungsverwaltung – www.geodaten.bayern.de“ |
| Österreich | DGM Österreich | 10 m | CC BY 4.0 | „Land Kärnten/geoland.at – data.gv.at“ |
| übrige Gebiete | Copernicus GLO-30 | 30 m | Copernicus-Lizenz | „© DLR e.V. 2010–2014 und © Airbus Defence and Space GmbH 2014–2018, bereitgestellt unter COPERNICUS durch die Europäische Union und ESA“ |

Für andere Alpenländer (Schweiz swissALTI3D, Italien TINITALY, Frankreich IGN, Slowenien
ARSO) nennt PLAN.md, Abschnitt 4.2, Lizenz und Text; Mapterhorn führt die vollständige Liste
in seiner `attribution.json`. Wer eine neue Region anlegt, trägt die zutreffenden Zeilen in
`regions/<id>.json` ein.

In der Testregion wird das Höhenmodell bis Zoom 13 ausgeliefert (etwa 6,5 m je Pixel); die
1-m-Daten Bayerns sind darin also gemittelt.

### Online-Karte

Der Stil „OpenFreeMap (online)“ lädt gehostete OpenMapTiles-Kacheln von openfreemap.org
(Daten ODbL). Er ist nur als Vergleich gedacht; alles andere arbeitet ohne Netz.

## Software

| Komponente | Zweck | Lizenz |
|---|---|---|
| MapLibre GL JS | Kartendarstellung | BSD-3-Clause |
| maplibre-contour | Höhenlinien aus dem Höhenmodell | BSD-3-Clause |
| Valhalla (Crate `valhalla`, Docker-Image für den Kachelbau) | Routing | MIT |
| Tauri | Desktop-Hülle | MIT / Apache-2.0 |
| SQLite (über `rusqlite`, mitgeliefert) | Datenbank | gemeinfrei |
| Planetiler, planetiler-openmaptiles | Bau der Vektorkacheln | Apache-2.0, BSD-3-Clause |
| go-pmtiles | Ausschneiden der Höhenkacheln | BSD-3-Clause |
| Noto Sans (`assets/map/fonts`) | Kartenbeschriftung | SIL Open Font License 1.1 |

Die Rust-Abhängigkeiten stehen in `Cargo.lock`, die des Frontends in
`apps/desktop/ui/pnpm-lock.yaml`.

## Was das für die Weitergabe heißt

- Regionspakete sind abgeleitete Datenbanken aus OpenStreetMap: Weitergabe unter ODbL mit
  Nennung; die Kartenbilder selbst sind „Produced Works“.
- Die Nennungen müssen sichtbar bleiben – die App zeigt sie in der Karte. Wer Bilder der
  Karte veröffentlicht, nennt OpenStreetMap und die Quelle des Höhenmodells.
- Hangneigung und Höhenprofile sind Rechenergebnisse aus einem Geländemodell, keine amtliche
  Auskunft und keine Lawinenbeurteilung.
