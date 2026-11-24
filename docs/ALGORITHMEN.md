# Algorithmen

Wie die Zahlen der App entstehen. Alles hier liegt im Rust-Kern (`crates/`), die Oberfläche
zeigt nur an. Herleitung, Alternativen und der Abgleich mit Komoot stehen in
[PLAN.md](../PLAN.md), Abschnitt 6.

## 1. Track-Statistik (`core-track`)

Reine Funktion `compute_stats(Track, StatsParams) → TrackStats`; dieselbe Funktion rechnet
während der Aufzeichnung, beim Import und in `track-cli`. Parameter je Aktivität:

| Parameter | Rad | MTB | Wandern | Skitour |
|---|---|---|---|---|
| Bewegungsschwelle `v_move` | 3,0 km/h | 2,0 km/h | 1,0 km/h | 0,8 km/h |
| `stop_delay_s` | 5 s | 5 s | 10 s | 10 s |
| `move_window_s` | 5 s | 5 s | 15 s | 15 s |
| `max_plausible_speed` | 110 km/h | 90 km/h | 25 km/h | 110 km/h |
| `ele_smooth_m` | 60 m | 40 m | 30 m | 30 m |
| `ele_threshold_m` | 3 m | 3 m | 3 m | 3 m |
| Ortungsintervall | 1 s | 1 s | 3 s | 3 s |

Zum Ausprobieren: `track-cli stats tour.gpx --set ele_threshold_m=5`.

### Filter (je Segment, in dieser Reihenfolge)

1. Ungültige Punkte fallen weg: Zeit nicht streng steigend, exakte Duplikate.
2. Punkte mit gemeldeter Genauigkeit schlechter als 50 m fallen weg.
3. **Sprungfilter:** Ein Punkt ist verdächtig, wenn die Geschwindigkeit vom letzten
   akzeptierten Punkt `max_plausible_speed` oder die Beschleunigung 6 m/s² übersteigt. Er
   bleibt trotzdem, wenn nur die Zeit nicht stimmt – die Durchschnittsgeschwindigkeit über
   das Fenster ist plausibel und der Punkt ist kein Zacken (hin und gleich zurück). Nach
   5 Verwerfungen in Folge gilt der nächste Punkt als neuer Anker; über ihn hinweg zählt
   keine Distanz.
4. **Stillstand:** Punkte näher als 3 m am letzten akzeptierten Punkt zählen nicht zur
   Distanz.

### Distanz, Geschwindigkeit, Zeit

- **Distanz:** Summe der Großkreisabstände (Haversine, Erdradius 6 371 008,8 m), ohne Höhe.
- **Geschwindigkeit je Punkt** über ein Zeitfenster. Meldet der Empfänger durchgehend
  Dopplergeschwindigkeit, gilt deren Median. Sonst: Nettoverschiebung über das Fenster für
  die Bewegungserkennung, zeitgewichteter Median der Einzelgeschwindigkeiten für die
  Höchstgeschwindigkeit (Fenster 10 s) – einzelne falsche Zeitstempel heben ihn nicht an.
- **Gesamtzeit:** je Segment letzter minus erster Zeitstempel; eine Pause beginnt ein neues
  Segment und zählt nicht.
- **Zeit in Bewegung:** Intervalle mit Geschwindigkeit ≥ `v_move`. Ein Abfall, der kürzer als
  `stop_delay_s` dauert, zählt als Bewegung, ein längerer ganz als Stillstand.
- **Lücken** über 30 s (Tunnel, kein Empfang): Die Strecke zählt; als Bewegungszeit die Zeit,
  die das Durchschnittstempo des übrigen Tracks dafür braucht, höchstens die Lücke selbst.

### Höhenmeter

1. Höhen über die **Wegstrecke** glätten (gleitender Mittelwert, `ele_smooth_m`) – unabhängig
   vom Aufzeichnungsintervall.
2. **Hysterese:** Im Anstieg zählt jeder neue Höchstwert sofort. Ein Richtungswechsel gilt
   erst, wenn die Höhe um `ele_threshold_m` vom letzten Extremwert abweicht. Gipfel und
   Senken gehen ganz ein, Rauschen unter der Schwelle gar nicht.
3. Niedrigster und höchster Punkt stammen aus den geglätteten Werten.

`core_track::elevation_gain` stellt Schritt 1 und 2 auch für beliebige Höhenprofile bereit
(geplante Routen).

## 2. Höhen aus dem Geländemodell (`core-maps`)

- Höhenkacheln sind Terrarium-kodiert: `h = R·256 + G + B/256 − 32768` Meter.
- `RegionStore::elevation(lat, lon)` nimmt die feinste Zoomstufe, die an der Stelle eine
  Kachel hat, und interpoliert **bilinear** zwischen den vier nächsten Pixelmitten.
  Dekodierte Kacheln bleiben in einem kleinen Cache.
- `elevation_profile(Linie, Schritt)` tastet eine Linie höchstens alle `Schritt` Meter ab;
  die Punkte der Linie selbst bleiben erhalten.

### Höhenkorrektur für Tracks (`core-app`, seit M6)

GPS-Höhen rauschen und driften. Öffnet man einen Track, rechnet die App die Statistik ein
zweites Mal mit Höhen aus dem Geländemodell und zeigt sie als „Aufstieg/Abstieg (Gelände)“
neben den Werten des Empfängers.

- Nur wenn das Modell für **mindestens 99 %** der Punkte eine Höhe hat; ein Gemisch beider
  Quellen würde Höhenmeter erfinden.
- Geglättet wird über die **doppelte** Wegstrecke (Rad 120 m statt 60 m), Schwelle wie sonst
  3 m. Modellhöhen rauschen nicht zeitlich, aber räumlich: Auf einer Straße quer zum Hang
  sind ein paar Meter seitlicher Versatz ein paar Meter Höhe.
- Der gespeicherte Track und der GPX-Export behalten die gemessenen Höhen.
- Grenze: Das Modell kennt Brücken, Tunnel und Galerien nicht – dort folgt es dem Gelände
  und erfindet Höhenmeter.

Abgleich an den beiden echten Radtouren (Aufstieg / Abstieg in Metern):

| | Höhen der Datei | Gelände, 1× glätten, 2 m | **Gelände, 2× glätten, 3 m** | Gelände, 3× glätten, 3 m | Komoot |
|---|---|---|---|---|---|
| Sylvensteinsee (Brücken, Straße im Steilhang) | 134 / 144 | 231 / 241 | **180 / 192** | 152 / 166 | 160 / 170 |
| Rottach-Egern → Obergries | 234 / 289 | 221 / 270 | **205 / 254** | 201 / 251 | 250 / 310 |

Die Tour am Sylvensteinsee zeigt die Grenze: Über die Seebrücken und am Steilufer liefert das
Modell deutlich mehr Höhenmeter als gefahren wurden, und das Ergebnis hängt stark von der
Glättung ab. Die zweite Tour ist stabil. Gewählt ist die Einstellung, die bei beiden
innerhalb von 20 % um Komoots Wert liegt; zwei Touren sind keine Eichung, die Werte sind
deshalb als zweite Meinung neben den GPS-Werten gedacht, nicht als deren Ersatz.

## 3. Routen (`core-routing`, `core-app`)

- **Teilstücke:** Eine Route ist eine Folge von Teilstücken zwischen je zwei Wegpunkten, jedes
  mit eigenem Profil. Jedes Teilstück ist eine eigene Anfrage an Valhalla, auf dem Graph der
  Region, die seine beiden Enden enthält. Berechnete Teilstücke (samt Höhenprofil) werden nach
  Region, Enden, Profil und Einstellungen gemerkt; eine Änderung rechnet nur die betroffenen
  neu. Ändert sich eine Region, wird alles vergessen. Ein Teilstück ohne Ergebnis zählt nicht
  mit und wird markiert; die übrigen bleiben.
- **Weg:** Profile und ihre Einstellungen:

  | Profil | Valhalla-Costing | Voreinstellung |
  |---|---|---|
  | Wandern | `pedestrian` | `max_hiking_difficulty` 3 (T1–T6 einstellbar), `use_hills` 0,8 |
  | MTB | `bicycle`, Typ `mountain` | `use_roads` 0,1, `avoid_bad_surfaces` 0, `use_hills` 0,8 |
  | Trekkingrad | `bicycle`, Typ `hybrid` | `use_roads` 0,3, `avoid_bad_surfaces` 0,4 |
  | Rennrad | `bicycle`, Typ `road` | `use_roads` 0,6, `avoid_bad_surfaces` 0,8 |
  | Auto | `auto` | Standard |
  | Skitour (manuell) | kein Router | gerade Linien, alle 50 m unterteilt |

- **MTB-Schwierigkeit:** Beim Bau des Graphs werden Wege mit `mtb:scale` über der Grenze
  (Standard 2) für Fahrräder gesperrt (`scripts/build-valhalla-tiles.sh --max-mtb-scale`).
  Valhalla speichert keine MTB-Schwierigkeit je Kante, eine Grenze pro Route gibt es deshalb
  nicht.
- **Höhenprofil:** Die Linie der Route wird alle 25 m im Geländemodell abgetastet – nicht mit
  Valhallas Höhen, damit Routen und Tracks dieselbe Quelle haben. Auf- und Abstieg nach dem
  Verfahren aus Abschnitt 1 (50 m glätten, 3 m Schwelle), je Teilstück; Etappe und Route
  sind die Summen. Fehlen Höhen auf einem Teilstück, hat es – und damit Etappe und Route –
  keine Höhenmeter. Ein Teilstück ohne Länge (beide Enden auf derselben Stelle) hat 0 m.
  An die Oberfläche geht das Profil auf etwa 2 000 Punkte ausgedünnt (Ende jedes Teilstücks
  bleibt erhalten); Speichern und GPX nutzen das volle Profil.
- **Zeit:** je Teilstück; Etappe und Route sind Summen. Zu Fuß (Wandern, Skitour) die
  DIN-Zeit, sobald Höhen bekannt sind, sonst Valhallas Zeit. Die DIN-Formel ist nicht
  additiv; je Teilstück gerechnet und summiert ist eine bewusste Näherung. **DIN 33466**:
  Zeit für die Strecke (4 km/h) und Zeit für die Höhe (300 Hm/h auf, 500 Hm/h ab) getrennt
  rechnen; die größere zählt ganz, die kleinere halb. Valhallas Gehzeit ist im Gebirge
  deutlich zu kurz (Lenggries → Brauneck: 2:05 h gegen 3:33 h).

## 4. Hangneigung (`tools/slope-tiles`)

- Je Kachel wird das Höhenraster mit 1 Pixel Rand aus den Nachbarkacheln geladen (keine
  Kanten an Kachelgrenzen).
- Gradient nach **Horn** (3×3). Die Pixelgröße hängt in Web-Mercator von der Breite ab und
  wird je Zeile berechnet: `156543,03 · cos(φ) / 2^z · 256 / Kachelgröße` Meter.
- Neigung `atan(√((dz/dx)² + (dz/dy)²))`, eingeteilt in 30–35° (gelb), 35–40° (orange),
  40–45° (rot), über 45° (violett); darunter durchsichtig.
- Gerechnet wird auf **einer** Zoomstufe (13, etwa 6,5 m je Pixel bei 47° N). Gröbere Stufen
  entstehen durch Mitteln, feinere durch Vergrößern in der Karte. Feiner zu rechnen ergäbe
  ein verrauschtes Bild, gröber verschluckt kurze Steilstufen – Neigung hängt vom Maßstab ab.
- Ausgabe als PNG mit Farbpalette; Kacheln ohne Gelände über 30° werden nicht gespeichert.

Die Hangneigung ist eine Planungshilfe. Sie ersetzt weder den Lawinenlagebericht noch die
Beurteilung im Gelände.

## 5. Aufzeichnung (`core-track`, `core-app`, `core-storage`)

- Zustandsautomat: Aufzeichnung ↔ Pause → Stopp; jede Fortsetzung beginnt ein neues Segment.
- Ein Fix wird übernommen, wenn seit dem letzten mindestens 80 % des Ortungsintervalls der
  Aktivität vergangen sind, und **sofort** in SQLite geschrieben (WAL). Nach einem Absturz
  fehlt höchstens der letzte Punkt; die App bietet „Fortsetzen“ oder „Abschließen“ an. Wer
  stattdessen neu startet, schließt die alte Aufzeichnung als eigenen Track ab.
- Die Oberfläche bekommt höchstens alle 250 ms einen Stand mit den neuen Linienpunkten.

## 6. Regionspakete (`core-maps`)

- Download dateiweise mit Fortsetzen (HTTP-Range), SHA-256-Prüfung jeder Datei, dann
  atomares Umbenennen des Verzeichnisses – eine Region ist ganz da oder gar nicht.
- Kacheln liest ein eigener PMTiles-v3-Leser; die Karte holt sie über das Schema `tiles://`
  direkt aus dem Rust-Kern.

## 6b. Aufzeichnung in Trail (`core-app`, `Inner::record`)

- **Ohne Sportart:** Trail startet jede Aufnahme als `other` („Keine Angabe“); die Sportart kommt
  beim Speichern (`set_track_activity`), danach wird die Statistik mit deren Werten neu gerechnet.
  `other` speichert einen Punkt je Sekunde wie Rad und MTB, denn es kann alles sein. Die
  Statistik nimmt ab 1 km/h Bewegung an und hält bis 110 km/h für plausibel.
- **GPS-Lücke:** Liegt ein Fix mehr als 120 s hinter dem letzten gespeicherten, beginnt ein neues
  Teilstück. Die Lücke zählt weder als Strecke noch als Bewegungszeit; ein Tunnel fehlt dadurch
  in der Strecke, dafür zählt keine Autofahrt mit ausgeschaltetem GPS. Importierte Tracks mit
  langen Pausen ohne Punkte werden beim Abspielen genauso geteilt.

## 7. Live-Routing (`core-app`, Modul `guide`)

Der Guide folgt der geplanten Route Fix für Fix: Fortschritt entlang der Linie, Abweichung,
Weg zurück, Rest. Er ändert die Route nie. Alle Werte (`GuideParams`) sind erste Werte, die mit
echten Tracks nachgestellt werden.

- **Fixes:** Genauigkeit über 50 m wird ignoriert. Abweichung: mehr als 40 m von der Linie, bei
  mindestens 3 Fixes in Folge und mindestens 10 s. Zurück auf der Route ist man unter 20 m.
  Angekommen: Fortschritt am Ende und höchstens 30 m von der Linie (Ziel in 30 m).
- **Suchfenster:** Der Fortschritt wird nur in einem Stück der Linie gesucht, damit Schleifen und
  nahe Parallelwege nicht springen: 200 m zurück, mindestens 500 m voraus. Nach einer Pause
  zwischen Fixes wächst es nach vorn um 20 m/s × 3 × Zeit seit dem letzten Fix, abseits der
  Route um dasselbe seit dem Verlassen. Bei Streckenabschnitten ohne Routing (Luftlinie, fehlend)
  reicht es bis hinter deren Ende.
- **Gleichstand:** Liegen zwei Stellen innerhalb von 1 m gleich nah (hin und zurück auf demselben
  Weg), gilt die, die dem bisherigen Fortschritt am nächsten liegt.
- **Gerade und fehlende Abschnitte** gelten nie als Abweichung: dort gibt es keinen Weg, dem man
  folgen könnte.
- **Weg zurück:** Bei einer Abweichung fragt der Kern das Routing nach einem Weg vom Fix zum
  Zielpunkt. Zielpunkt ist der nächste Wegpunkt voraus. Solange man noch nie auf der Route war,
  ist es der Wegpunkt, an dem der Guide platziert wurde: aus der Ferne also der Start; bei „Ab
  hier führen“ (`guide_from_waypoint`) genau dieser Wegpunkt. Das Profil ist das des Abschnitts
  am Fortschritt, sonst das des letzten gerouteten davor, sonst des ersten danach; nie eine
  Luftlinie. Die Fahrtrichtung (Kurs) wird nur ab 1,5 m/s übergeben, darunter ist sie zu
  verrauscht. Schlägt es fehl (`no_way_back`), wird nach 60 s oder 200 m Bewegung neu versucht.
- **Rest:** Strecke und Zeit bis zum Etappenende und zum Ziel, mit dem Rest des Wegs zurück
  eingerechnet. Die Zeit ist `None`, sobald ein Abschnitt voraus keine Zeit hat.
- **Aufzeichnung:** Führung und Aufzeichnung sind unabhängig (Entscheidung des Nutzers nach dem
  ersten Test): Keine startet oder beendet die andere. Eine neue Route setzt eine laufende
  Führung neu auf, startet aber keine.
- **Meldungen an die App:** Der Listener erfährt die Änderungen der Führung in der Reihenfolge,
  in der sie geschehen; das Ende kommt nie vor einem Status, der davor entstand. Aus
  `on_guidance` heraus darf er die Führung weder starten noch stoppen und keinen Fix einspeisen.
- **Hinweis in Trail:** Je Abweichung (wenn `deviation` wächst) eine Vibration, im Hintergrund dazu
  eine Mitteilung „Abseits der Route“ mit „Weg zurück zu Wegpunkt {n}: {Distanz}.“ bzw. „Weg zurück
  zum Ziel: {Distanz}.“. Scheitert der Weg zurück schon bei der Abweichung, lautet der Text „Kein
  Weg zurück gefunden.“: Auch dann soll der Nutzer merken, dass er abseits ist. Die Distanz ist die
  ganze Länge des Wegs zurück im Moment der Abweichung; die Leiste zeigt sie unverändert, bis die
  Abweichung endet. Den gefahrenen Teil der Route misst Trail wie der Kern (Haversine,
  `EARTH_RADIUS_M`).
