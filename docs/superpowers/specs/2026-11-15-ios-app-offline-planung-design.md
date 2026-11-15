# Entwurf: iOS-App für Routenplanung offline, mit Freigabe im Heimnetz

Stand: 2026-11-15 · Status: zur Freigabe

## Anlass und Ziel

Phase 2 beginnt mit iOS. Android kommt später, weil es gerade kein Android-Testgerät gibt.
Die **Hauptmotivation** des Nutzers: unterwegs **ohne Netz Routen planen und umplanen**, auf
einer **Topokarte mit Hangneigung**. Zitat: „unterwegs neu planen, umplanen muss ich dauernd,
und genau hier versagen oft online-only Routings“. Zu Hause plant er am Desktop.

Der Vorabtest (`docs/superpowers/spikes/2026-11-15-ios-valhalla-spike.md`) hat das größte
Risiko ausgeräumt. `Rallista/valhalla-mobile` 0.6.4 bringt Valhalla 3.9.1 mit, liest unsere
`routing.tar` direkt und liefert auf einem iPhone 13 Pro Max im Flugmodus dieselben Routen
wie der Desktop (auf den Meter genau, 1–3 ms je Route im Median).

Ziel: eine iOS-App, die Regionen und Routen aus dem Heimnetz vom Desktop übernimmt und danach
ohne Netz Routen genauso plant wie die Desktop-App.

## Ziele

1. **Karte offline:** Stil Sommer und Winter wie am Desktop, Hangneigung mit einstellbarer
   Deckkraft, Höhenlinien, Schummerung. Dazu die **eigene Position als Punkt**, ohne
   Aufzeichnung.
2. **Routenplanung wie am Desktop:** Wegpunkte durch Tippen setzen, verschieben und entfernen.
   Sportart und Einstellungen für das letzte und alle folgenden Teilstücke wählen. Wegpunkte als
   Etappenende markieren. Rückgängig und Leeren. Die Teilstücke sind nach Profil gefärbt, ein
   fehlgeschlagenes ist gestrichelt. Höhenprofil farbig nach Profil, mit Statistik gesamt und je
   Etappe (Distanz, Auf- und Abstieg, Höhen, Zeit). Die Logik dafür liegt schon im Rust-Kern
   (`crates/core-app/src/plan.rs`, `routes.rs`). Die iOS-App bedient sie nur.
3. **Routen speichern, laden, umbenennen und löschen** auf dem iPhone. **„Speichern“
   überschreibt die geladene Route** (gleiche `id`, `rev` + 1), **„Als neue Route speichern“**
   legt eine neue an (Nutzerentscheidung). Das gilt auch am Desktop, weil die Logik im
   gemeinsamen Kern liegt. Heute legt `Storage::save_route` immer eine neue Zeile an. Der Kern
   braucht dafür eine Funktion, die `plan_json` und die Kennzahlen einer gespeicherten Route
   ersetzt, und der Planungszustand merkt sich, welche gespeicherte Route geladen ist. Am
   Desktop kommt der Knopf „Als neue Route speichern“ neben „Speichern“.
4. **Freigabe im Heimnetz am Desktop:** ein Schalter „Im Heimnetz freigeben“. Ein kleiner,
   nur lesender Webserver liefert die installierten Regionen (Katalog und Pakete) und die
   gespeicherten Routen aus. Die Desktop-App zeigt Adresse und QR-Code.
5. **Übernahme auf dem iPhone:** QR-Code scannen oder Adresse eintippen. Danach Regionen aus
   diesem Katalog installieren, aktualisieren und löschen, über den vorhandenen Download mit
   Prüfsummen. Gespeicherte Routen vom Desktop übernehmen.
6. **Höhenlinien vorberechnet:** `scripts/build-region.sh` erzeugt sie als zusätzliche Datei
   im Paket, weil MapLibre Native sie nicht selbst berechnen kann.

## Nicht-Ziele (zurückgestellt, nicht gestrichen)

- Aufzeichnen, Pausieren und Fortsetzen von Tracks, Hintergrundortung. Laut Nutzer „später
  wichtig“. Die Anbindung des Kerns soll das nicht verbauen: Der `LocationSource`-Trait
  bleibt push-basiert, Swift liefert die Positionen.
- GPX-Export und -Import auf dem iPhone, und Routen vom iPhone zurück zum Desktop. Das kommt
  später über GPX oder über den Abgleich in Phase 3.
- Android.
- Ein dauerhaft laufender Server (Nutzerentscheidung). Die Freigabe läuft nur, solange der
  Schalter in der Desktop-App an ist.
- Regionen auf dem iPhone selbst bauen.
- Abbiegehinweise und Navigation mit Sprachansagen (Nicht-Ziel aus PLAN.md).

## Anti-Ziele

- **Kein Schritt der Planung darf ein Netz brauchen.** Ein Rückgriff auf Online-Routing oder
  Online-Karten, sobald die Region installiert ist, wäre genau das Versagen, das die App
  beheben soll. Der Online-Stil des Desktops kommt auf iOS nicht vor.
- Die Logik für Planung, Statistik und mehrere Regionen entsteht **nicht ein zweites Mal in
  Swift**. Swift ist eine dünne Schicht für Oberfläche, Karte, Ortung und Valhalla-Aufruf.

## Rahmenbedingungen

- **Zwei Rechner:** Rust-Kern, Bauskript und Freigabe entstehen auf dem Linux-Rechner (Ubuntu
  26.04, dort liegen Planetiler, Docker und die Quelldaten). Die Swift-App entsteht auf dem
  MacBook Air (Xcode 27.2, Rust für iOS eingerichtet), in einer eigenen Claude-Code-Sitzung.
  Übergabe zwischen beiden nur über das Git-Repo. Das Gedächtnis der Linux-Sitzung ist auf dem
  Mac nicht sichtbar, also muss alles Nötige in Repo-Dokumenten stehen.
- Testgerät iPhone 13 Pro Max, iOS 27.2, dazu der Simulator. Signieren mit dem kostenlosen
  „Personal Team“ (Profil gilt 7 Tage). Mindestversion iOS 16.4 (wegen valhalla-mobile).
- valhalla-mobile 0.6.4 = Valhalla 3.9.1 = Version, mit der das Bauskript die Routingdaten
  baut. Die App prüft beim Laden `routing.json` (`"valhalla"`). Weicht die Version ab, ist
  die Region zwar als Karte nutzbar, aber nicht zum Routen, mit einer klaren Meldung.
- Desktop-Rechner: GNOME/Wayland, mehrere Netzwerke (WLAN `192.168.2.x`, WireGuard, rund zehn
  Docker-Bridges). `ufw` ist aktiv. Den Port freizugeben braucht einmal `sudo`, das kann die
  App nicht.
- Kartendaten bis 10 GB auf dem Handy sind in Ordnung (Nutzerentscheidung aus Vorhaben 2).
- Arbeitsweise: nach jedem Meilenstein anhalten und dem Nutzer zeigen; Push erst nach einem
  Meilenstein, einmal vorher fragen. Der Nutzer testet Bedienung von Hand; Selbsttests treiben
  echte Eingaben (Tippen und Ziehen), keine internen Aufrufe.

## Ansätze

### Valhalla auf iOS (entschieden durch den Vorabtest)

- **Gewählt: valhalla-mobile in der Swift-Schicht.** Der Rust-Kern baut die Anfrage
  (`costing_json`) und wertet die Antwort aus (`parse_valhalla_route`). Swift reicht nur den
  JSON-Text an `route(rawRequest:)` durch. Für iOS wird `core-routing` ohne Feature `valhalla`
  gebaut, Boost und Protobuf müssen nie für iOS übersetzt werden.
- Verworfen: das Rust-Paket `valhalla` für iOS übersetzen. Unklarer Aufwand, und nicht nötig.

### Wie die Karte an die Kacheln kommt (Nutzer wählte A)

- **A, gewählt: wie am Desktop.** Swift hängt eine eigene URL in MapLibres Netzwerkzugriff
  ein (eigenes Schema oder `URLProtocol` in der Session-Konfiguration von
  `MLNNetworkConfiguration`) und fragt jede Kachel beim Rust-Kern ab (`App::map_tile`). Die
  Styles und das Zusammenführen mehrerer Regionen bleiben dieselben wie am Desktop
  (PLAN.md 2.5). Ob MapLibre Native iOS das Einhängen sauber erlaubt, ist offen. Deshalb ist
  es der **erste Schritt** im Bauplan.
- B, Rückfall: MapLibre liest die PMTiles direkt (`pmtiles://file://`). Dann braucht jede
  Region eigene Quellen und doppelte Ebenen, und es gäbe Logik für mehrere Regionen in Swift.
  Nur wenn A nicht geht.
- C, verworfen: ein Webserver auf `localhost` im Rust-Kern auf dem iPhone. Port in der App,
  iOS hält ihn im Hintergrund an.

### Wie Regionen aufs iPhone kommen (Nutzer wählte die Freigabe im Heimnetz)

- **Gewählt: Freigabe im Heimnetz.** Sie nutzt den vorhandenen Katalog-Download aus
  `core-maps` (HTTP, Prüfsummen, Fortschritt) unverändert, und sie war ohnehin als Vorhaben 2
  geplant.
- Verworfen: Import über die Dateien-App oder AirDrop. 100–600 MB je Region von Hand, kein
  bequemes Aktualisieren.
- Verworfen: eigener Server auf dem VPS. Phase 3, und ein dauerhafter Server ist vorerst nicht
  gewollt.

### Routen Desktop → iPhone (Nutzer: ja, über die Freigabe)

- **Gewählt: Die Freigabe liefert auch die gespeicherten Routen.** Das iPhone übernimmt sie
  auf Wunsch. Das ist wenig Aufwand, weil der Server ohnehin entsteht.
- Verworfen: getrennte Planungen ohne Übergabe. Dann müsste jede zu Hause geplante Tour auf
  dem Handy neu gesetzt werden.

## Aufbau

```
iPhone                                              Desktop (Linux)
┌──────────────────────── SwiftUI ───────────────────┐    ┌── Tauri-App ─────────────┐
│ Karte (MapLibre Native) ── Kachel-URL ──┐          │    │ Schalter „Freigeben“     │
│ Planung, Höhenprofil, Listen            │          │    │ Adresse + QR-Code        │
│ CLLocationManager (nur Punkt)           │          │    │ Webserver (nur lesen):   │
│ valhalla-mobile ◄── JSON ──┐            │          │    │  catalog.json, Pakete,   │
└─────────────────────────────┼───────────┼──────────┘    │  Routen                  │
                UniFFI        │           │               └────────────▲─────────────┘
┌───────── Rust-Kern (core-app, ohne Feature valhalla) ────┐           │
│ Router-Trait ── Callback an Swift ──┘   map_tile()       │           │
│ Planung, Statistik, Routen speichern (SQLite)            │ ── HTTP im Heimnetz ──┘
│ RegionStore: installieren über Katalog-URL, Prüfsummen   │
└───────────────────────────────────────────────────────────┘
```

### Rust-Kern für iOS

- **Neue Hülle `crates/core-ffi`** (UniFFI 0.32, siehe PLAN.md Tabelle) mit
  `crate-type = ["staticlib", "lib"]`. Sie gibt die Kern-API von `core-app` als
  UniFFI-Objekte und -Typen aus. Die Tauri-Commands bleiben eine eigene Hülle daneben. Die
  Hülle enthält keine Fachlogik.
- **Router von außen:** `core-app` öffnet Router heute über `open_router` (mit Feature
  `valhalla` oder gar nicht). Neu: Wer `App` anlegt, kann eine Router-Fabrik übergeben (Pfad
  der `routing.tar` → `Arc<dyn Router>`). Auf iOS liefert Swift sie per UniFFI-Callback. Sie
  hält eine valhalla-mobile-Instanz je Region, legt sie einmal je App-Start an und benutzt
  sie wieder (das Öffnen kostet etwa 100 ms) und ruft sie nie aus zwei Threads zugleich auf.
  Der Kern ruft dann `costing_json` → Callback → `parse_valhalla_route`. Der Desktop behält
  seinen bisherigen Weg.
- **Build:** ein Skript erzeugt das XCFramework für `aarch64-apple-ios` und
  `aarch64-apple-ios-sim` sowie die Swift-Bindings. Es läuft auf dem Mac. Die CI-Prüfung
  `cargo check --target aarch64-apple-ios` bleibt und deckt `core-ffi` mit ab.
- Valhalla-Ausgaben auf der Konsole und die Warnung zur fehlenden `traffic.tar` werden in der
  valhalla-mobile-Konfiguration abgestellt (Stolperstein aus dem Vorabtest).

### Karte auf iOS

- Dieselben Style-Dateien aus `styles/`. Die Platzhalter `__TILES__` und `__FONTS__` ersetzt
  Swift durch die eigene Kachel-URL, `bounds` durch die Grenzen der installierten Regionen, wie
  `offlineStyle()` in `apps/desktop/ui/src/map.ts`. Ohne installierte Region: nur der
  Hintergrund und ein Hinweis, wie man Regionen holt.
- **Höhenlinien:** Auf iOS zeigt die Quelle `contours` auf die neue Ebene `contours` aus dem
  Paket (siehe unten) statt auf `__CONTOURS__`. Quell-Ebene `contours` mit den Feldern
  `ele` und `level`, wie sie der Style heute schon erwartet. Damit bleiben die Ebenen im Style
  gleich. Der Desktop bleibt bei `maplibre-contour` (keine Änderung in diesem Vorhaben).
- **Symbole:** Am Desktop zeichnet `drawIcon` in `map.ts` die Piktogramme erst bei Bedarf
  (`styleimagemissing`). Auf iOS übernimmt das die Delegate-Methode für fehlende Bilder von
  MapLibre Native, mit denselben Formen und Farben, in Swift gezeichnet.
- **Schriften:** Die Glyphen-Dateien der Desktop-App kommen ins App-Bundle und werden über
  dieselbe Kachel-URL ausgeliefert (`/fonts/<font>/<range>.pbf`).
- Route, Wegpunkte, Etappenenden und fehlgeschlagene Teilstücke werden genauso gezeichnet wie
  am Desktop: weißer Rand, Farbe nach Profil aus `profile-colors.ts`, Etappenende
  hervorgehoben.
- Die Profilfarben stehen heute nur in TypeScript. Sie wandern in den Kern (eine Funktion
  Profil → Farbe), damit beide Oberflächen dieselben Farben zeigen.

### Bedienung auf dem iPhone

Ein Bildschirm mit der Karte. Darunter ein Bereich, der sich hochziehen lässt (Sheet, wie in
Apple Karten), mit drei Stufen: eingeklappt (Kurzstatistik), halb (Profil, Einstellungen,
Etappen), ganz (Höhenprofil groß, Listen). Oben eine Leiste mit Stilwahl (Sommer/Winter),
Deckkraft der Hangneigung und „Position zentrieren“.

```
┌──────────────────────────┐
│ [Sommer|Winter] ◐  ⌖     │
│                          │
│     Karte mit Route      │
│   ①───②━━━③ (MTB)        │
│                          │
├──────── ▔▔▔ ─────────────┤
│ 23,4 km  ↑1 210 m  6:40 h │
│ [Wandern ▾] [+ Punkt] ↶ ⌫ │
└──────────────────────────┘
```

- **Planen:** Der Knopf „+ Punkt“ schaltet in den Planmodus, ähnlich wie „Bearbeiten“ am
  Desktop. Dann setzt Tippen auf die Karte einen Wegpunkt. Außerhalb des Planmodus verschiebt
  und zoomt Tippen nur die Karte, damit kein Punkt aus Versehen entsteht.
- Ziehen eines Wegpunkts verschiebt ihn. Tippen auf einen Wegpunkt öffnet ein Menü:
  „Etappenende hier“ (nur innere Punkte) und „Entfernen“, wie das Wegpunktmenü am Desktop.
- Das Sportart-Menü gilt wie am Desktop für das letzte und alle folgenden Teilstücke. Die
  Regler der Einstellungen kommen aus der Profilbeschreibung des Kerns (`route_profiles`).
- Höhenprofil: Ein Finger auf dem Profil zeigt die Stelle als Marke auf der Karte, wie der
  Profil-Cursor am Desktop.
- Listen „Routen“ (gespeichert auf dem iPhone, mit „vom Desktop übernehmen“) und „Regionen“
  im ganz ausgezogenen Sheet.
- Texte auf Deutsch, wie am Desktop. Die Texte des Kerns (Fehlercodes) werden auf iOS
  übersetzt.

### Freigabe im Heimnetz (Desktop)

- Ein Schalter im Bereich „Regionen“. An: Ein Webserver lauscht auf einem festen Port (Vorschlag
  `8765`, in den Einstellungen änderbar) auf allen IPv4-Adressen. Er liefert nur aus, schreibt
  nie.
  - `GET /catalog.json`: ein Katalog im heutigen Format (`core-maps` `Catalog`), erzeugt aus
    den **installierten** Regionen. Paket-URLs relativ, damit der vorhandene Download sie
    findet. Der Testeintrag `fixture-tegernsee` (nur Manifest) wird übersprungen, weil seine
    Dateien fehlen. Allgemein gilt: Regionen ohne vollständige Dateien liefert der Server nicht
    aus.
  - `GET /<region>/<datei>`: die Dateien der Region, nur die, die im Manifest stehen. Kein
    Ausliefern anderer Pfade.
  - `GET /routes.json`: Liste der gespeicherten Routen (`id`, `name`, `profile`,
    `distance_m`, `ascent_m`, `updated_at_ms`, `rev`).
  - `GET /routes/<id>.json`: der vollständige Plan (`plan_json`) samt Metadaten.
- **Adresse:** Die Desktop-App zeigt `http://<adresse>:<port>/` und den QR-Code dazu. Als
  Adresse nimmt sie die private IPv4-Adresse der Schnittstelle mit der Standardroute,
  **nicht** WireGuard- oder Docker-Adressen. Gibt es mehrere Kandidaten, zeigt sie eine
  Auswahl.
- **Firewall:** Kann die App nicht feststellen, ob der Port offen ist, zeigt sie einen Hinweis
  mit dem Befehl zum Kopieren (`sudo ufw allow from 192.168.0.0/16 to any port 8765
  proto tcp`, angepasst an das erkannte Netz).
- Die Freigabe endet, wenn der Schalter aus geht oder die App schließt. Ein Download, der
  gerade läuft, bricht dann ab. Der Download in `core-maps` setzt danach per `Range`-Anfrage
  fort (`fetch.rs`), deshalb muss der Server `Range` für die Paketdateien beherrschen.
- Kein Passwort: Das Heimnetz gilt als vertrauenswürdig. Ausgeliefert werden nur Kartendaten
  und Routen, nur lesend.

### Übernahme auf dem iPhone

- **Quelle hinzufügen:** QR-Code scannen (Kamera, VisionKit) oder Adresse eintippen. Die App
  merkt sich die Adresse als Katalogquelle. Bei einer neuen IP genügt ein neuer Scan.
- **Regionen:** Liste aus `catalog.json` mit Größe, installiert/aktualisierbar/nicht
  installiert. Installieren, aktualisieren und löschen über `RegionStore`, wie am Desktop.
  Speicherort im Application-Support-Verzeichnis, vom iCloud-Backup ausgenommen (die Daten
  lassen sich jederzeit neu laden).
- **Routen:** Liste aus `routes.json`. „Übernehmen“ legt die Route mit **derselben `id`** auf
  dem iPhone an. Gibt es sie dort schon:
  - Hat sich die iPhone-Fassung seit der letzten Übernahme nicht geändert: Sie wird durch die
    Desktop-Fassung ersetzt.
  - Hat der Nutzer sie auf dem iPhone geändert: Die Desktop-Fassung kommt als **neue Route**
    dazu, mit dem Namenszusatz „(Desktop)“. Es geht nichts verloren.
  - Dafür merkt sich das iPhone je übernommener Route den `rev`-Stand des Desktops und den
    eigenen `rev` danach. Die Spalten `rev`/`synced_rev` gibt es schon.
  - Wurde die Route auf dem iPhone gelöscht (Zeile mit `deleted_at`), holt „Übernehmen“ sie mit
    der Desktop-Fassung zurück. Der Nutzer hat ausdrücklich übernommen. (Meine Wahl.)
- iOS-Rechte: `NSLocalNetworkUsageDescription` (Zugriff aufs Heimnetz),
  `NSAllowsLocalNetworking` in den App-Transport-Einstellungen (unverschlüsseltes HTTP im
  lokalen Netz), `NSCameraUsageDescription` (QR-Code),
  `NSLocationWhenInUseUsageDescription` (Punkt).
- Downloads laufen weiter, solange die App im Vordergrund ist. Herunterladen im Hintergrund ist
  nicht nötig.

### Höhenlinien im Paket

- Neuer Schritt `contours` in `scripts/build-region.sh`, nach `dem`. Er erzeugt
  `contours.pmtiles` (Vektorkacheln, Quell-Ebene `contours`, Felder `ele` in Metern und
  `level` 0 = Neben-, 1 = Hauptlinie) aus `dem.pmtiles`. Dabei gelten dieselben Abstände wie
  am Desktop, Zoom 11: 100/500 m, 12: 50/250 m, 13: 20/100 m, ab 15: 10/50 m (`map.ts`,
  `CONTOUR_TILES`). Höchster Zoom **14** (Nutzerentscheidung nach Messung in I2: z15 hätte
  die Datei verdreifacht, ohne echtes Detail, weil die Höhendaten bei z13 enden). Darüber
  vergrößert die Karte, also 20/100 m statt 10/50 m ab Zoom 15.
- Das Werkzeug ist meine Wahl: ein Rust-Werkzeug `tools/contour-tiles` neben
  `tools/slope-tiles`. Es liest dieselben Terrarium-Kacheln, und die PMTiles schreibt
  `core-maps` (`writer.rs`), wie bei der Hangneigung. Keine neue Abhängigkeit im Bauskript.
- `Layer` in `core-maps` bekommt `Contours`, optional wie `Slope`. Ältere Pakete ohne
  Höhenlinien gehen weiter, nur ohne Linien auf iOS.
- Die beiden vorhandenen Pakete (`isarwinkel-achensee`, `suedtirol`) werden um den Schritt
  ergänzt (`--steps contours,manifest`). Ein kompletter Neubau ist nicht nötig.
- Der Größenzuwachs wird gemessen und in `docs/MESSPROTOKOLL.md` eingetragen.

## Meilensteine

Reihenfolge nach Risiko. Nach jedem wird angehalten und gezeigt.

| | Inhalt | Wo | Fertig, wenn |
|---|---|---|---|
| **I1** | `core-ffi` mit UniFFI. Router-Fabrik von außen. iOS-App-Gerüst. Karte über die eigene Kachel-URL aus dem Kern (Ansatz A geprüft). Eigene Position. Region per Hand ins Simulator-/App-Verzeichnis kopiert. | Linux + Mac | Sommer- und Winterkarte mit Hangneigung offline auf dem iPhone. Punkt sichtbar. |
| **I2** | Höhenlinien im Bauskript, beide Pakete ergänzt. | Linux | `contours.pmtiles` in beiden Paketen. Höhenlinien auf dem iPhone sichtbar. |
| **I3** | Routenplanung auf iOS komplett: Wegpunkte, Teilstücke, Etappen, Höhenprofil, Statistik, Speichern und Laden. | Mac (+ Linux für Kern-Änderungen) | Selbsttest mit echten Tipp- und Zieh-Eingaben: Er tippt die drei Referenzpunkte aus dem Vorabtest an ihrer Bildschirmstelle (Valhalla rastet sie am Wegenetz ein). Die Wanderroute weicht höchstens 1 % von 12 353 m ab (meine Wahl). Laden, Umplanen, Speichern überschreibt dieselbe Route. Der Nutzer plant im Flugmodus von Hand. |
| **I4** | Freigabe im Heimnetz am Desktop. Übernahme von Regionen und Routen auf dem iPhone per QR-Code. | Linux + Mac | Auf einem frisch installierten iPhone kommen eine Region und eine Desktop-Route per QR-Code aufs Gerät. Danach wird im Flugmodus weitergeplant. |

I4 ist für das Ziel nötig, wird aber nach I3 gebaut. Bis dahin kommen Regionen per Hand aufs
Gerät (Xcode/Finder-Dateifreigabe), wie im Vorabtest.

## Messwerte, die festgehalten werden

App-Größe, Start bis Karte sichtbar, Dauer je neues Teilstück auf dem iPhone,
Downloadgeschwindigkeit einer Region im WLAN, Größe von `contours.pmtiles`. Alles in
`docs/MESSPROTOKOLL.md`.

## Dem Bauenden überlassen

- Die genaue UniFFI-Form der API (Objekte, Fehler, Callback für Ereignisse) und wie das
  XCFramework gebaut wird.
- Wie die eigene Kachel-URL in MapLibre Native eingehängt wird (Schema oder `URLProtocol`),
  solange Ansatz A gilt.
- Der Aufbau des Swift-Projekts (xcodegen wie im Vorabtest ist naheliegend). Ordner:
  `apps/ios/`.
- Der Webserver am Desktop (kleine Bibliothek, nur lesend, eigener Thread). Die Bibliothek für
  den QR-Code.
- Wie das Höhenprofil in SwiftUI gezeichnet wird (Swift Charts oder eigenes Zeichnen), solange
  Farben, Etappenlinien und der Cursor wie am Desktop sind.
- Die genaue Gestaltung des Sheets, im Rahmen der Skizze oben.
