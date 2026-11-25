# Trail: Abbiegehinweise und Sprachansagen

Stand 2026-11-25. Baut auf dem Live-Routing auf (`2026-11-24-aufzeichnung-live-routing-design.md`, in `main` seit PR #7). Die Builder-UI in Base bleibt weiter zurückgestellt.

## Absicht

Trail kann Routen planen, aufzeichnen und auf einer Route führen. Damit die Führung eine runde Sache wird, fehlen nur noch Hinweise vor Abzweigungen. Der Nutzer will sich unterwegs an jeder Abzweigung sagen lassen, wohin es geht („rechts“), ohne auf die Linie schauen zu müssen. Wahlweise soll das auch bei gesperrtem Bildschirm und akustisch gehen.

So nutzt er Trail (aus dem Entwurf zum Live-Routing): Beim Radfahren hängt das iPhone am Lenker, der Bildschirm ist oft aus und nur bei Bedarf an. Beim Wandern ist er meist aus. Auf mehrtägigen Touren gibt es keine Steckdose, **der Akkuverbrauch ist ein Hauptziel der ganzen App.**

## Ziele

- Vor jeder echten Abzweigung ein kurzer Hinweis, zweimal: eine Vorankündigung und einer unmittelbar davor.
- Der Zeitpunkt hängt von der Sportart ab: beim Wandern feste Abstände, bei Rad und Auto mit dem Tempo skaliert.
- Der Hinweis erscheint während der Führung als Feld oben in der App und verschwindet von selbst, sobald er nicht mehr gilt.
- **Optional:** der Hinweis als Mitteilung, wenn die App im Hintergrund läuft oder der Bildschirm gesperrt ist.
- **Optional:** Sprachansage der Hinweise.

## Nicht-Ziele (später)

- Hinweise für Routen ohne gespeicherte Abzweigungen (vor allem ältere gespeicherte und geteilte Routen). Mit Map-Matching (`trace_route` in Valhalla) ließe sich das nachrüsten, ohne die Linie zu verändern. Der Nutzer: Alte Routen sind derzeit „komplett irrelevant“.
- Straßennamen, Hinweistafeln, Fahrspuren, Ausfahrtsnummern auf der Autobahn.
- Hinweise auf gerade verbundenen Teilstücken (Profil `Manual`): Dort gibt es keine Abzweigungen.
- Hinweise in Base. Base rechnet dieselben Abzweigungen mit, zeigt aber nichts an.
- Einstellbare Abstände oder Stimmen.

## Was auf keinen Fall passieren darf

- Lange Sätze. Der Nutzer will **„definitiv nur kurze Ansagen“**: „In 200 m rechts“, „Jetzt rechts“, nicht „Biegen Sie rechts ab auf die Hauptstraße“.
- Ein Hinweis bleibt stehen, der nicht mehr stimmt: nach der Abzweigung, abseits der Route, nach einer Neuplanung.
- Mitteilungen sammeln sich in der Mitteilungszentrale an.
- Trail meldet sich an jeder Wegkreuzung, obwohl es geradeaus weitergeht.
- Etwas braucht Internet, oder Daten verlassen das Gerät. Die Sprachausgabe nutzt nur die Stimme auf dem Gerät.
- Mitteilungen oder Sprache sind an, ohne dass der Nutzer sie eingeschaltet hat.
- Die Hinweise verändern die geplante Route.

## Ansatz

Betrachtet wurden:

- **A: Valhalla liefert die Abzweigungen beim Planen (gewählt).** Heute fragt der Kern Valhalla mit `directions_type: "none"` an (`crates/core-routing/src/valhalla_api.rs`). Mit `"maneuvers"` kommen Art, Lage auf der Linie (`begin_shape_index`) und bei Kreisverkehren die Ausfahrt mit. Der Kern speichert das je Teilstück in der Route. Das Nachrüsten für alte Routen über Map-Matching gehörte ursprünglich zu A und ist auf später verschoben (siehe Nicht-Ziele).
- B: Nur neu geplante Routen bekommen Hinweise, alte bekommen einen Satz „neu berechnen“. Im Ergebnis entspricht das A ohne Nachrüstung, nur ohne Aufforderung zum Neuberechnen, denn Neuberechnen kann die Linie verändern.
- C: Abzweigungen aus dem Knick der Linie ablesen. Verworfen: Eine kurvige Straße sähe aus wie eine Abzweigung, eine flache Gabelung würde übersehen, Kreisverkehre gar nicht erkannt.

Wie beim Live-Routing entscheidet der Rust-Kern, welcher Hinweis wann gilt (Entscheidung von Claude). Trail zeigt ihn nur an, schickt die Mitteilung und spricht. So lässt sich die Logik unter Linux mit abgespielten GPX-Tracks testen.

## Vom Nutzer entschieden

- Hinweise vor Abzweigungen. Beim Wandern ein fester Abstand, bei Rad und Auto mit dem Tempo skaliert.
- Ein Textfeld oben in der App, das von selbst verschwindet.
- Mitteilung im Hintergrund und auf dem Sperrbildschirm sowie Sprachansage, beides optional.
- Nur kurze Hinweise, keine Straßennamen.
- Ansatz A, alte Routen derzeit irrelevant.
- Nach der Prüfung des Entwurfs: Abzweigungen an Wegpunkten aus den Richtungen der Teilstücke, „geradeaus“ mit deutlichem Winkel als Abzweigung, „Etappenziel erreicht“, Sprache auch bei Stummschaltung (Vorschläge von Claude, vom Nutzer bestätigt).
- Das Verhalten unten (Abschnitte „Was angesagt wird“ bis „Sprachansage“) hat Claude vorgeschlagen, der Nutzer hat es bestätigt („passt so“).

## Was es heute gibt

- **Planen:** `plan_route` in `core-app/src/plan.rs`. Ein `RoutePlan` (`PLAN_FORMAT = 2`) hat je Teilstück ein `Leg` mit Linie, Profil, Strecke und Zeiten. Valhalla rechnet auf dem Gerät, `parse_valhalla_route` liest nur `shape` und `summary`.
- **Führen:** `core-app/src/guide/` (Fortschritt auf der Linie, Abweichung, Weg zurück). Der Kern meldet Swift den Stand über `GuidanceInfo` (`core-ffi/src/guide.rs`): Phase, `along_m`, nächster Wegpunkt, Reststrecken. Nach dem Test draußen ersetzt Trail beim Abkommen das aktive Teilstück durch eine neue Route ab der Position (`onReroute` → `PlanController.reroute`). Diese neue Route kommt aus `plan_route` und hat deshalb auch Abzweigungen.
- **Trail:** `GuideController`, `GuideBar` (Leiste mit Reststrecken), `DeviationAlert` (Vibration, lokale Mitteilung, Schalter „Signalton“). `UIBackgroundModes` enthält nur `location`.
- Profile: `RoadBike`, `Trekking`, `Mtb`, `Hike`, `Car`, `Manual`.

## Abzweigungen im Kern

- Valhalla wird mit `directions_type: "maneuvers"` angefragt. Aus jedem Manöver übernimmt der Kern Art, Lage auf der Linie des Teilstücks und bei Kreisverkehren die Nummer der Ausfahrt. Die Lage wird von Valhallas Index in die zusammengefügte Linie des Teilstücks umgerechnet, denn `parse_valhalla_route` entfernt doppelte Punkte an den Grenzen.
- Der Kern übersetzt das in eine eigene, kleine Liste von Arten. Valhallas Typen gehen nicht nach außen:

| Art | Kurztext | Aus Valhalla |
|---|---|---|
| leicht links/rechts | „leicht links“ | slight left/right |
| links/rechts | „links“ | left/right |
| scharf links/rechts | „scharf links“ | sharp left/right |
| wenden | „wenden“ | u-turn left/right |
| links/rechts halten | „links halten“ | stay left/right, fork, ramp/merge links oder rechts |
| Kreisverkehr | „Kreisverkehr, 2. Ausfahrt“ | roundabout enter (mit `roundabout_exit_count`) |
| Ziel | „Ziel erreicht“ (nur „jetzt“) | destination am letzten Teilstück |
| Etappenziel | „Etappenziel erreicht“ (nur „jetzt“) | Wegpunkt am Ende einer Etappe |

- **Nicht angesagt:** geradeaus weiter (außer mit deutlichem Winkel, siehe unten), „wird zu“, Start, das Verlassen des Kreisverkehrs.
- **Übersehene Abzweigungen** (vom Nutzer bestätigt): Valhalla fasst Wege mit gleichem oder ohne Namen manchmal zusammen und meldet „geradeaus weiter“ über eine echte Kreuzung hinweg, vor allem auf Wander- und MTB-Wegen. Ein solches Manöver mit deutlichem Winkel (Valhallas `turn_degree`, Anhaltspunkt ab etwa 45°) gilt als Abzweigung der passenden Art. Die Schwelle wird an abgespielten echten Tracks geprüft und in `docs/ALGORITHMEN.md` festgehalten.
- **Abzweigungen an Wegpunkten** (vom Nutzer bestätigt): Der Kern lässt jedes Teilstück einzeln rechnen (`route_on` mit zwei Punkten in `plan.rs`). Valhalla sieht an einem inneren Wegpunkt deshalb nur Ziel und Start, nie die Richtungsänderung. Das betrifft Wegpunkte auf Kreuzungen und jede Naht nach einem Umplanen. Nur an diesen Nähten bestimmt der Kern die Abzweigung aus den Richtungen der beiden Linien (die letzten bzw. ersten Meter der Teilstücke): geradeaus kein Hinweis, sonst die passende Art, bei fast entgegengesetzter Richtung „wenden“. Die Linien und das Routing der Teilstücke bleiben unverändert. Naht zu oder von einem Teilstück `Manual` oder einem gescheiterten: kein Hinweis.
- **Etappen:** Am Ende einer Etappe „Etappenziel erreicht“ (nur „jetzt“), am letzten Wegpunkt „Ziel erreicht“. Andere innere Wegpunkte werden nicht angesagt, nur eine Abzweigung dort. Welche Valhalla-Typen genau wohin fallen, legt der Bauende fest und hält es in `docs/ALGORITHMEN.md` fest.
- Ein `Leg` bekommt ein Feld mit den Abzweigungen. `PLAN_FORMAT` steigt auf 3. Gespeicherte Routen im Format 2 lassen sich weiter laden, nur ohne Abzweigungen. Ob das Feld fehlt oder leer ist, muss unterscheidbar bleiben: Ein Teilstück ohne Abzweigung ist etwas anderes als eines, für das keine berechnet wurde.
- Teilstücke `Manual` und gescheiterte Teilstücke haben keine Abzweigungen.

## Was angesagt wird und wann

- **Zwei Hinweise je Abzweigung:** eine Vorankündigung („In 200 m rechts“) und einer unmittelbar davor („Jetzt rechts“).
- **Die Sportart** kommt vom Profil des Teilstücks, auf dem der Nutzer gerade ist.
- **Zeitpunkt:** Bei Rad und Auto zählen die Sekunden bis zur Abzweigung beim aktuellen Tempo laut GPS. Das Profil setzt Untergrenzen in Metern, damit die Hinweise beim Halt an einer Ampel nicht zu spät kommen. Beim Wandern gelten feste Abstände. Anhaltspunkte, wie in ähnlichen Apps üblich (die genauen Werte legt der Bauende fest und hält sie in `docs/ALGORITHMEN.md` fest):

| Profil | Vorankündigung | Unmittelbar davor |
|---|---|---|
| `Hike` | 50 m | 10–15 m |
| `RoadBike`, `Trekking`, `Mtb` | 20–30 s, mindestens etwa 100 m | 5–8 s, mindestens 30 m |
| `Car` | 30–40 s, mindestens 400 m; ab etwa 80 km/h 1–2 km | 8–10 s, mindestens 100 m |

- **Tempo:** geglättet über die letzten Sekunden, ungenaue Punkte zählen nicht. Ohne brauchbares Tempo gilt eine übliche Geschwindigkeit des Profils.
- **Dicht beieinander:** Folgt die nächste Abzweigung kurz nach der ersten, etwa innerhalb des Abstands für „jetzt“ der nächsten, kommen beide in einen Hinweis: „Jetzt rechts, dann gleich links“. Der Bauende legt die Schwelle fest.
- **Kurz nach dem Start oder nach einer Neuplanung** liegt die erste Abzweigung schon näher als die Vorankündigung. Dann kommt gleich der passende Hinweis, keine verspätete Vorankündigung.
- Jeder Hinweis kommt **einmal**. Steht der Nutzer an einer Abzweigung oder sorgen GPS-Sprünge für ein Hin und Her, wiederholt sich nichts.
- **Abstand im Text:** gerundet, wie andere Navi-Apps: unter 100 m auf 10 m, darüber auf 50 m, ab 1 km auf 0,1 km.

## Anzeige in der App

Ein Feld oben, über der Führungsleiste, mit großem Pfeil und dem Abstand, der herunterzählt (Skizze, Gestaltung im Stil der vorhandenen Leisten, im OLED-Dunkelmodus mit):

```
┌──────────────────────────────────────┐
│  ↱   200 m   rechts                  │
├──────────────────────────────────────┤
│ Ziel: 12,4 km · 0:48 h        [◎][✕] │
└──────────────────────────────────────┘
```

- Das Feld erscheint mit der Vorankündigung und bleibt bis zur Abzweigung stehen. Der Abstand zählt mit jedem Punkt herunter.
- Es verschwindet, sobald der Nutzer an der Abzweigung vorbei ist, bei einer Abweichung (dann gilt die Anzeige „Abseits der Route“) und bei einer Neuplanung, bis die neue Route ihren ersten Hinweis hat.
- Bei jedem neuen Hinweis vibriert das iPhone kurz, schwächer als bei einer Abweichung (Entscheidung von Claude).
- Hat die Route keine Abzweigungen (Format 2), zeigt die Führungsleiste einmal „Diese Route hat keine Abbiegehinweise“. Das gilt nicht für Teilstücke `Manual`.

## Mitteilung im Hintergrund und auf dem Sperrbildschirm

- Ein Schalter in den Einstellungen, **standardmäßig aus**. Jede Mitteilung schaltet den Bildschirm kurz ein, das kostet Akku.
- Ist der Schalter an und die App nicht im Vordergrund, schickt Trail zu jedem Hinweis eine lokale Mitteilung. Die nächste ersetzt sie über dieselbe Kennung, es steht immer höchstens eine Abbiegemitteilung da.
- Ist die Abzweigung vorbei, die Führung beendet oder der Nutzer abseits der Route, entfernt Trail die Mitteilung wieder.
- Der Ton folgt dem vorhandenen Schalter „Signalton“. Die Erlaubnis für Mitteilungen gibt es schon vom Live-Routing.

## Sprachansage

- Ein eigener Schalter, **standardmäßig aus**.
- Ist er an, spricht Trail jeden Hinweis mit demselben kurzen Text, im Vordergrund, im Hintergrund und bei gesperrtem Bildschirm. Dazu kommt `audio` in die `UIBackgroundModes`.
- `AVSpeechSynthesizer` mit einer deutschen Stimme auf dem Gerät, ohne Internet.
- **Auch bei stumm geschaltetem iPhone** (vom Nutzer bestätigt), wie bei Google Maps: Wer die Sprache eigens einschaltet, will sie hören. Also Kategorie `.playback` mit `.duckOthers` (bzw. `.voicePrompt`-Modus), nicht `.ambient`. Die Audiositzung duckt andere Wiedergabe (Musik wird leiser) und gibt sie danach wieder frei. Sie ist nur aktiv, solange etwas gesprochen wird.
- Bei eingeschalteter Sprache wird auch die Abweichung gesprochen („Abseits der Route“) und das Ziel („Ziel erreicht“).
- Ist bei eingeschalteter Sprache auch der Mitteilungsschalter an, kommt die Mitteilung ohne Ton, damit es nicht doppelt klingelt (Entscheidung von Claude).
- Ein neuer Hinweis unterbricht einen noch laufenden alten.

## Akku

Die Hinweise kosten fast nichts: Der Fortschritt auf der Linie wird ohnehin je Punkt berechnet, die nächste Abzweigung ist ein Nachschlagen. Valhalla rechnet die Abzweigungen beim Planen in einem Durchgang mit. Mitteilungen und Sprache sind standardmäßig aus, weil sie den Bildschirm wecken oder die Audiositzung öffnen.

## Dem Bauenden überlassen

- Wie die Hinweise zu Swift kommen: als Teil von `GuidanceInfo` (etwa nächste Abzweigung, Abstand, Stufe „vorab“/„jetzt“, Zähler für neue Hinweise wie bei `deviation`) oder als eigene Nachricht des Listeners.
- Genaue Abstände, Glättung des Tempos, Schwelle für „dicht beieinander“, Zuordnung der Valhalla-Typen. Begründet mit abgespielten echten Tracks, festgehalten in `docs/ALGORITHMEN.md`.
- Pfeilsymbole (SF Symbols) und die genaue Gestaltung von Feld und Einstellungen.
- Ob die Texte im Kern oder in Swift entstehen. Sie sollen an einer Stelle liegen und für Anzeige, Mitteilung und Sprache gleich sein.

## Prüfung

- **Kern unter Linux:** Valhalla-Antworten mit Manövern zerlegen (Beispiele als Testdaten); Umrechnung der Lage über zusammengefügte Teilstücke; Format 2 lädt weiter. Mit abgespielten GPX-Tracks: Zeitpunkte bei Wandern, Rad und Auto, langsamer und schneller Fahrt, Halt vor der Abzweigung, zwei Abzweigungen dicht hintereinander, Kreisverkehr, Abzweigung an einem Wegpunkt und an der Naht nach einem Umplanen, Weg ohne Namen mit Abzweig, Etappenende, Abweichung und Neuplanung, erste Abzweigung kurz nach dem Start, GPS-Sprünge (kein doppelter Hinweis).
- **Trail im Simulator:** Feld erscheint und verschwindet mit simulierter Bewegung; Mitteilung wird ersetzt und entfernt; Sprache nur bei Schalter an.
- **Draußen** (der Nutzer): eine Runde mit dem Rad und eine zu Fuß, mit eingeschaltetem und gesperrtem Bildschirm, je mit und ohne Sprache, einmal mit Stummschalter. Zu prüfen: Kommen die Hinweise rechtzeitig und nicht doppelt, verschwinden Feld und Mitteilung, wird Musik leiser und danach wieder lauter? Ergebnis in `docs/MESSPROTOKOLL.md`.
