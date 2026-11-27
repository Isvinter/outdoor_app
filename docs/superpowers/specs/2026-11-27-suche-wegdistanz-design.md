# Suche: echte Wegdistanz zu den Treffern (Trail)

Stand 2026-11-27, im Brainstorming mit dem Nutzer entworfen.

## Absicht

Die Suche in Trail zeigt und sortiert Treffer heute nach Luftlinie. Unterwegs ist ein naher
Punkt aber oft nur über einen großen Umweg erreichbar, und das sieht man erst, wenn man eine
Route dorthin plant. Künftig soll die Trefferliste auf Wunsch die **echte Weglänge und Zeit**
für die eingestellte Sportart zeigen und danach sortieren.

Weil das Rechenzeit und damit Akku kostet (Akku ist Kernziel der App, mehrtägige Touren ohne
Strom), ist die Funktion abschaltbar.

## Ziele

- Mit GPS-Position und eingeschaltetem Umschalter: Luftlinie **und** Weg werden von der eigenen
  Position gemessen, die Liste ist nach Wegstrecke sortiert (Nutzer).
- Treffer über 50 km Luftlinie von der Position werden nicht geroutet (Nutzer; Beispiel: in
  München Mailand im Kartenausschnitt, dort POIs suchen → keine Routen).
- Die Liste springt nicht um: die Wege werden berechnet, bevor die Liste erscheint.
- Umschalter direkt in der Suchleiste, der sich seinen letzten Zustand merkt (Nutzer).

## Nicht-Ziele

- Keine Linien der Wege auf der Karte; „Route hierher“ bleibt der Weg zur echten Route.
- Keine Änderung an Index, Suchabfrage, Suchradius (sichtbarer Kartenausschnitt, 2–50 km)
  oder Kartenanpassung (`fit`).
- Keine Wege für die weit entfernten Orte außerhalb des Radius.

## Anti-Ziele

- Eine Suche, die spürbar langsamer wird oder bei jedem Tastendruck lange rechnet.
- Akkuverbrauch, wenn der Umschalter aus ist: dann muss alles exakt wie heute laufen.

## Verhalten

| Lage | Distanz gemessen von | Sortierung |
|------|----------------------|------------|
| Umschalter aus | Kartenmitte (wie heute) | wie heute |
| Umschalter an, keine GPS-Position | Kartenmitte (wie heute) | wie heute |
| Umschalter an, GPS-Position | eigene Position | siehe unten |

Mit Position gilt wie heute zuerst die Treffergüte (`match_rank` in `best_first`): ein exakter
Namenstreffer bleibt vor einem halben (Nutzer). Innerhalb gleicher Güte – bei Kategoriesuchen
haben alle dieselbe – gilt diese Reihenfolge:
1. Treffer mit Weg, nach Wegstrecke aufsteigend.
2. Treffer bis 50 km Luftlinie ohne Weg (Valhalla findet keinen, oder Ziel außerhalb der
   Routing-Region), nach Luftlinie, mit dem Hinweis „Umweg oder kein Weg“.
3. Treffer über 50 km Luftlinie von der Position, nach Luftlinie, ohne Hinweis.
4. Die weit entfernten Orte außerhalb des Radius bleiben in ihren reservierten Plätzen am
   Ende (wie `share_limit` heute), Luftlinie von der Position.

Eine Zeile mit Weg zeigt Wegstrecke und Zeit; die Luftlinie klein daneben:

```
[ 🔍 Camping            ] [⇄ Weg]
  Campingplatz Seeblick   3,2 km · 0:52   Luftl. 1,1 km
  Camping Waldrand        4,0 km · 1:05
  Zeltplatz Insel         0,8 km          Umweg oder kein Weg
```

Zeiten kommen für alle Sportarten aus der Matrix, auch beim Wandern (Nutzer). Valhallas
Gehzeit berücksichtigt die Höhenmeter kaum und weicht damit von der DIN-Gehzeit ab, die
„Route hierher“ zeigt (`Shared::measure`, `crates/core-app/src/plan.rs`). Beim Profil `hike`
steht deshalb ein kleiner Hinweis an der Liste, etwa „Zeiten ohne Höhenmeter“. Der Nutzer
plant Touren ohnehin über das Höhenprofil; die Zeit dient vor allem für die Stadt
(Einkaufen, Restaurant suchen, Abendbummel zu Fuß auf Radreisen).

## Ansätze

- **A – Distanzmatrix von Valhalla (gewählt).** Ein Aufruf `Actor::matrix` (in `valhalla`
  0.7.6 vorhanden, `src/bindings/rust/src/actor.rs:185`) rechnet von der Position zu allen
  Zielen in einem Durchgang Länge und Zeit, ohne Linie. Schnell und akkuschonend; Mehraufwand
  ist ein zweiter Valhalla-Aufruf im Router.
- **B – Einzelne Routen mit dem vorhandenen `Router::route`.** Nichts Neues am Router, aber bis
  zu 30 Rechnungen hintereinander, bei 50 km evtl. über eine Sekunde und mehr Akku. Verworfen.
- **Nachladen nach dem Erscheinen der Liste.** Liste springt um; verworfen.

Der Nutzer hat A gewählt.

## Aufbau

### Rust-Kern

- `core-routing`: `Router` bekommt eine Methode für eine Quelle und viele Ziele, z. B.
  `fn matrix(&self, request: &MatrixRequest) -> Result<Vec<Option<Leg>>, RoutingError>` mit
  Länge (m) und Zeit (s) je Ziel, `None` = kein Weg. Standard-Implementierung im Trait gibt
  `RoutingError::Unavailable` zurück, damit JSON-Router, `manual` und Test-Router nichts tun
  müssen. `ValhallaRouter` setzt sie mit `Action::SourcesToTargets` um, die Kosten-JSON kommt
  aus derselben Quelle wie beim Routen (`costing_json`: Profil und `RouteOptions`).
- `core-app`: eine neue Funktion neben `search`/`category` in `crates/core-app/src/search.rs`,
  die fertige `SearchResults` plus Position, Profil und Optionen nimmt und sie wie oben neu
  ordnet. `SearchHit` bekommt `route_m: Option<f64>`, `route_s: Option<f64>` und ein Kennzeichen
  „geprüft, kein Weg“. Die angezeigte Luftlinie kommt mit Position von der Position; die
  Luftlinie zur Kartenmitte bleibt als eigenes Feld erhalten, weil `fit` und die reservierten
  Plätze der fernen Orte (`distanceM <= radiusM`) sie weiter brauchen.
  Routing-Kacheln: Region über `routing_tiles` für die Position wählen; Ziele außerhalb ihrer
  bbox gelten als „kein Weg“. Profil `manual` oder keine Routing-Daten → nur Luftlinie von der
  Position, keine Wege.
- `core-ffi`: die neue Funktion samt den neuen Feldern nach Swift reichen
  (`crates/core-ffi/src/search.rs`).

### Swift (Trail)

- `SearchController.run`: Ist der Umschalter an und eine Position bekannt, ruft derselbe
  Hintergrundjob nach der Suche die neue Kernfunktion auf; erst dann `show(results)`.
  `fit` nutzt weiterhin die Lage der Treffer zum Suchradius der Kartenmitte.
- Position: dieselbe Quelle wie „Route hierher“ (`RouteHereHost.position`, das eine GPS der
  App), höchstens so alt wie dort erlaubt (`MAX_POSITION_AGE_S`, 120 s). Die Suche schaltet
  das GPS nicht selbst ein.
- Wird der Umschalter bei offener Suche umgelegt, läuft die Suche neu.
- Sportart und Einstellungen: was eine neue Etappe bekäme (`PlanController.formLeg`), also
  dasselbe, womit „Route hierher“ rechnet.
- Umschalter in der Suchleiste (`SearchBar.swift`), Zustand in `UserDefaults`.
- Ergebniszeile (`SearchOverlay`/Trefferliste) zeigt Wegstrecke · Zeit, Luftlinie, „kein Weg“.

## Risiko: Rechenzeit bei 50 km

Gemessen sind nur Routen bis 12 km (2,6–5 ms auf dem iPhone). Valhalla rechnet eine Matrix mit
einer Quelle als Ausbreitung um die Quelle (Dijkstra-artig), deren Aufwand mit dem
entferntesten Ziel wächst, nicht mit der Zahl der Ziele. Zu Fuß über 50 km kann das deutlich
teurer sein als eine einzelne A*-Route. **Erster Bauschritt ist deshalb eine Messung** auf
Desktop und iPhone: Matrix von einem Punkt zu ~30 Zielen bei 10, 25 und 50 km, für `hike`,
`trekking` und `mtb`. Liegt sie auf dem iPhone deutlich über ~300 ms, wird mit dem Nutzer
neu entschieden (z. B. kleinere Grenze je Sportart) – die 50 km sind seine Entscheidung.
Valhallas `service_limits` (Matrix-Distanz und -Paare) für die Profile prüfen und ggf. in
`ConfigBuilder` anheben.

## Nachtrag 2026-11-27: Messung und Suchgrenze

Gemessen auf `alpenraum-ost` (1,4 GB), 30 Ziele auf einem Ring um Bozen, Median: Wandern
264 ms, MTB 232 ms, Trekking **2173 ms** bei 50 km (334 ms bei 25 km). Teuer sind die Ziele
mit großem Umweg (50 km Luftlinie, 84–88 km Weg): Valhalla muss alles näher Liegende absuchen.
Valhallas CostMatrix ist fast überall langsamer (Trekking 50 km 1412 ms).

**Entscheidung des Nutzers (Variante A):** Die Matrix sucht nur bis zu einer festen Grenze
(Valhallas `max_matrix_distance` je Profil), eingestellt auf etwa 300 ms im ungünstigen Fall.
Ziele, die sie bis dahin nicht erreicht, zeigen „Umweg oder kein Weg“ statt „kein Weg“
(Valhalla unterscheidet „zu weit“ nicht von „unerreichbar“). Begründung des Nutzers: Bei
Bikepacking und Trekking in Alpen, Apennin und Pyrenäen träfe er diesen Fall oft; dann zählen
Karte und Höhenprofil, die Liste ist nur grobe Orientierung – Warten wäre Akkuverschwendung.

## Meine Entscheidungen (vom Nutzer nicht ausdrücklich bestätigt)

- Alle Treffer im Radius werden berechnet (Liste hat höchstens 30), nicht nur die ersten x.
- Sortiert wird nach Wegstrecke, nicht nach Zeit.
- Umschalter ist nach der Installation **aus**.
- „Luftl.“ steht in jeder Zeile mit Weg klein daneben.
- Unterhalb der 50 km ohne Weg: Hinweis „Umweg oder kein Weg“; darüber kein Hinweis.

## Dem Bauenden überlassen

- Genaue Namen und Typen der neuen Kern-Schnittstelle.
- Layout des Umschalters und der Zeile im Rahmen der Skizze.
- Ob die Matrix in den bestehenden seriellen Such-Queue läuft (Vorgabe: ja, im selben Job).

## Tests

- Rust: Neuordnung mit einem Test-Router, der feste Matrixwerte liefert (alle vier Gruppen,
  `manual`, keine Routing-Daten, Ziel außerhalb der Region). Ein Valhalla-Test gegen
  `fixtures`-Kacheln, wie die bestehenden Router-Tests.
- Swift: `SearchControllerTests` mit falschem Backend: Umschalter aus → Aufruf wie heute;
  an ohne Position → wie heute; an mit Position → Kernfunktion gerufen, Liste erst danach.
- Von Hand auf dem iPhone: Umweg-Fall, Messung der Rechenzeit.
