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
  Segment und zählt nicht. Bei Abschnitten (unten, 6b) zählt auch die Zeit vom letzten Fix eines
  Abschnitts zum ersten des nächsten, wenn dieser ihn fortsetzt (Sportwechsel, GPS-Lücke, Rast,
  Rastende), nicht nach Pause/Weiter oder Fortsetzen nach Absturz: Die Tour läuft vom ersten
  zum letzten Fix ohne die Pausen.
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
- Die Oberfläche bekommt höchstens alle 250 ms einen Stand mit den neuen Linienpunkten;
  beginnt oder endet eine Rast, sofort (die App drosselt danach das GPS).
- **Abschnitte:** Jedes Segment ist ein Abschnitt mit eigener Sportart und dem Grund seines
  Beginns (Start, Sportwechsel, Weiter, GPS-Lücke, Rast, Rastende, Fortsetzen). Ein Sportwechsel
  während der Aufnahme beginnt mit dem nächsten Fix einen neuen Abschnitt, der im Intervall der
  neuen Sportart aufzeichnet. Rast, Pause und Lücke behalten die Sportart. `tracks.activity` ist
  die Sportart des ersten Abschnitts.
- **Statistik je Abschnitt:** Jeder Abschnitt wird mit den Parametern seiner Sportart gerechnet
  (`compute_section_stats`), die Tour ist die Summe (Höchstgeschwindigkeit und Höhen: Maximum
  bzw. Minimum). Eine Rast zählt nur zur Gesamtzeit, nicht zu Bewegungszeit, Strecke oder
  Höhenmetern. Der Abschnitt nach einer Rast bekommt zusätzlich die Gerade vom ersten Fix der
  Rast (dem Erkennungsfix am Rastplatz) zu seinem ersten Fix als Strecke: Während der Rast sind
  die Fixe grob oder selten, die letzten schon auf dem Weg, so ginge dieses Stück verloren.
  Mit Geländehöhen gilt dasselbe je Abschnitt.
- **Rasterkennung** (`RestDetector`, im Kern standardmäßig aus, Trail schaltet sie ein):
  Voreinstellung 5 min innerhalb von 50 m. Vor der Rast zählen nur brauchbare Fixe: gemeldete
  Genauigkeit ≤ Radius oder unbekannt. Der erste brauchbare Fix nach einem Zurücksetzen ist der
  Rastpunkt; ein brauchbarer Fix weiter als der Radius davon wird der neue Rastpunkt. Die Rast
  beginnt mit dem ersten brauchbaren Fix im Radius mindestens 5 min nach dem Rastpunkt, in einem
  eigenen Abschnitt; die Fixe davor bleiben, wo sie sind. Während der Rast zählt jeder Fix, mit
  seiner Genauigkeit als Spielraum: „weg“ ist ein Fix weiter als Radius + Genauigkeit (0, wenn
  unbekannt). Zwei Weg-Fixe nacheinander beenden die Rast (Abschnitt Rastende ab dem zweiten);
  ein Fix im Spielraum dazwischen setzt die Zählung zurück, ein einzelner Sprung beendet nichts.
  Pause, Sportwechsel und Weiter setzen die Erkennung zurück. Wird die Erkennung während einer
  Rast geändert oder ausgeschaltet, endet die Rast: Der nächste Fix beginnt einen Abschnitt
  Rastende. Während einer Rast bricht eine
  Funkstille über 120 s keine GPS-Lücke: Das gedrosselte GPS meldet im Stand oft minutenlang
  nichts.

## 6. Regionspakete (`core-maps`)

- Download dateiweise mit Fortsetzen (HTTP-Range), SHA-256-Prüfung jeder Datei, dann
  atomares Umbenennen des Verzeichnisses – eine Region ist ganz da oder gar nicht.
- Kacheln liest ein eigener PMTiles-v3-Leser; die Karte holt sie über das Schema `tiles://`
  direkt aus dem Rust-Kern.

## 6b. Aufzeichnung in Trail (`core-app`, `Inner::record`)

- **Ohne Sportart:** Trail startet jede Aufnahme als `other` („Keine Angabe“); die Sportart kommt
  beim Speichern (`set_track_activity`, setzt alle Abschnitte außer Rasten), danach wird die
  Statistik mit deren Werten neu gerechnet. Einzelne Abschnitte ändert `set_section_activity`.
  `other` speichert einen Punkt je Sekunde wie Rad und MTB, denn es kann alles sein. Die
  Statistik nimmt ab 1 km/h Bewegung an und hält bis 110 km/h für plausibel.
- **GPS-Lücke:** Liegt ein Fix mehr als 120 s hinter dem letzten gespeicherten, beginnt ein neues
  Teilstück (Grund „Lücke“, außer während einer Rast). Die Lücke zählt weder als Strecke noch als
  Bewegungszeit, wohl aber zur Gesamtzeit; ein Tunnel fehlt dadurch
  in der Strecke, dafür zählt keine Autofahrt mit ausgeschaltetem GPS. Importierte Tracks mit
  langen Pausen ohne Punkte werden beim Abspielen genauso geteilt.
- **Fotos und Notizen** (`core-app`, Modul `media`): Ein Medium unterwegs bekommt die Zeit des
  Aufrufs, den Ort des letzten Fixes und den **letzten gespeicherten Abschnitt mit einem Punkt**:
  den laufenden, während einer Rast den Rast-Abschnitt, während einer Pause den vorigen, nach
  einem Sportwechsel bis zum nächsten Fix noch den vorigen, nach dem Fortsetzen einer
  abgebrochenen Aufnahme den letzten gespeicherten; vor dem ersten Punkt die ganze Tour. So
  entsteht nie ein leerer Abschnitt nur für ein Foto. Gespeichert wird die UUID des Segments,
  nicht seine Position; der Index für die Anzeige folgt aus der Reihenfolge der Segmente.
  Nachträglich eingefügte Bilder und Notizen ohne Zeit bekommen den ersten Zeitpunkt ihres
  Abschnitts (bei der ganzen Tour den des Tracks). Eine Aufnahme ohne Punkt, aber mit Medien,
  wird beim Beenden behalten statt verworfen.

## 7. Live-Routing (`core-app`, Modul `guide`)

Der Guide folgt der geplanten Route Fix für Fix: Fortschritt entlang der Linie, Abweichung,
Weg zurück, Rest. Er selbst ändert die Route nie; das tut Trail (siehe „Umplanen“). Alle Werte (`GuideParams`) sind erste Werte, die mit
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
- **Umplanen (Trail, nach dem Test draußen vom Nutzer gewünscht):** Ist ein Weg zurück gefunden,
  ersetzt Trail die Route ohne Rückfrage: von der Position (Anfang des Wegs zurück) zum Zielpunkt
  des Wegs zurück und von dort wie bisher. Der gefahrene Teil und die erreichten Wegpunkte
  verschwinden; wer ihn sehen will, zeichnet auf. Das neue erste Teilstück behält das Profil des
  ersetzten, die übrigen kommen aus dem Zwischenspeicher. Einmal je Abweichung. Die Führung wird
  auf der neuen Route neu angesetzt und läuft weiter. Die Route gilt danach nicht mehr als die
  gespeicherte (`detach_planned_route`): „Speichern“ legt eine neue an, die gespeicherte bleibt.
  Ohne Weg zurück bleibt die Route, und die Leiste sagt „Kein Weg zurück gefunden“.
- **Abseits des Wegenetzes (nach dem Test draußen vom Nutzer gewünscht):** Umgeplant wird nur auf
  dem Wegenetz, still und ohne Hinweis; danach gelten die Abbiegehinweise der neuen Route. Ob der
  Nutzer auf dem Netz ist, zeigt der Weg zurück: Das Routing beginnt ihn auf dem nächsten Weg, der
  für das Profil taugt. Liegt sein Anfang mehr als 25 m (`on_network_m`) vom Fix entfernt (GPS-
  Fehler und Wegbreite) oder gibt es keinen Weg zurück, ist der Nutzer abseits des Netzes, etwa
  weglos im Wald. Dann bleibt die Route, und `off_network_seq` wächst einmal; weitere Wege zurück
  während desselben Ausflugs zählen nicht neu. Kommt der Nutzer dem Weg zurück auf 10 m nahe
  (`back_on_network_m`, kleiner als 25 m, damit GPS-Rauschen nicht hin und her schaltet), ist er
  wieder auf dem Netz, und Trail plant von seiner Position aus um (nicht vom Anfang des Wegs
  zurück, an dem er inzwischen vorbei sein kann). Zurück auf der Route endet der Zustand.
  Solange er gilt, nennt der Kern Luftlinie und Richtung zum nächsten Punkt der Route ab 200 m vor
  dem Fortschritt: „Die Route liegt 120 m bei 150°“, die Richtung auf 10° gerundet (Norden 0°),
  die Distanz wie bei den Abbiegehinweisen.
- **Rest:** Strecke und Zeit bis zum Etappenende und zum Ziel, mit dem Rest des Wegs zurück
  eingerechnet. Die Zeit ist `None`, sobald ein Abschnitt voraus keine Zeit hat.
- **Aufzeichnung:** Führung und Aufzeichnung sind unabhängig (Entscheidung des Nutzers nach dem
  ersten Test): Keine startet oder beendet die andere. Eine neue Route setzt eine laufende
  Führung neu auf, startet aber keine.
- **Meldungen an die App:** Der Listener erfährt die Änderungen der Führung in der Reihenfolge,
  in der sie geschehen; das Ende kommt nie vor einem Status, der davor entstand. Aus
  `on_guidance` heraus darf er die Führung nicht aufrufen, auch nicht nach dem Status fragen (das
  kann sich mit einem anderen Thread verklemmen); der Status kommt mit dem Aufruf. Trail folgt nur
  diesen Meldungen, nicht den Rückgabewerten von Start und Stopp: Die Meldungen hängen auf dem
  Hauptthread hinterher, und ein Status nach dem Stopp sähe sonst wie ein neuer Start aus.
- **Hinweis in Trail:** Nur abseits des Wegenetzes (wenn `off_network_seq` wächst) eine
  Vibration, im Hintergrund dazu eine Mitteilung „Abseits der Route“ mit „Die Route liegt 120 m
  bei 150°.“ und „Weg zurück zu Wegpunkt {n}: {Distanz}.“ bzw. „Weg zurück zum Ziel: {Distanz}.“
  bzw. „Kein Weg zurück gefunden.“. Bei eingeschalteter Sprache sagt Trail „Abseits der Route. Die
  Route liegt 120 m bei 150 Grad.“; die Leiste zeigt Luftlinie und Richtung laufend. Die Distanz ist die
  ganze Länge des Wegs zurück im Moment der Abweichung; die Leiste zeigt sie unverändert, bis die
  Abweichung endet. Den gefahrenen Teil der Route misst Trail wie der Kern (Haversine,
  `EARTH_RADIUS_M`).

## 7b. Abbiegehinweise (`core-routing`, `core-app`, Modul `guide`)

Der Kern entscheidet, welcher Hinweis gilt; Trail zeigt ihn nur, vibriert, schickt eine Mitteilung
oder spricht ihn, sobald `turn_seq` wächst. Die Hinweise verändern die Route nie. Alle Werte sind
erste Werte nach dem Vorbild ähnlicher Apps und werden mit abgespielten echten Tracks nachgestellt.

- **Abzweigungen von Valhalla** (`valhalla_api.rs`, Nummern von `DirectionsLeg_Maneuver_Type`,
  Valhalla 3.x): 9/16 leicht rechts/links, 10/15 rechts/links, 11/14 scharf rechts/links, 12 und 13
  wenden; rechts halten bei Rampe, Ausfahrt, „bleiben“ und Einfädeln nach rechts (18, 20, 23, 37),
  links halten entsprechend (19, 21, 24, 38); 26 Kreisverkehr mit `roundabout_exit_count` als
  Ausfahrt (ohne bekannte Ausfahrt nur „Kreisverkehr“). „Wird zu“, „geradeaus weiter“, Rampe
  geradeaus, „geradeaus bleiben“ und Einfädeln (7, 8, 17, 22, 25) gelten nur als Abzweigung, wenn
  die Linie dort um mindestens 45° abknickt: Valhalla fasst Wege mit gleichem oder ohne Namen
  zusammen und meldet sonst „geradeaus“ über echte Kreuzungen, vor allem auf Wander- und
  MTB-Wegen. Alles andere (Start, Ziel, Verlassen des Kreisverkehrs, Fähre …) wird nicht angesagt.
  Die Lage wird von Valhallas Index in die Linie ohne doppelte Punkte an den Grenzen umgerechnet.
- **Winkel und Reichweite** (`turns.rs`): Die Richtung vor und nach einem Punkt wird zwischen dem
  Punkt und dem Punkt etwa 20 m davor bzw. dahinter auf der Linie gemessen (oder dem Ende der
  Linie, wenn sie kürzer ist); so zählen Knicke einzelner Stützpunkte nicht. Unter 30° geradeaus
  (kein Hinweis), 30–60° leicht, 60–120° normal, 120–160° scharf, ab 160° wenden.
- **Nähte an Wegpunkten** (`line.rs`): Jedes Teilstück wird einzeln geroutet, Valhalla sieht an
  einem inneren Wegpunkt nur Ziel und Start. Zwischen zwei gerouteten Teilstücken bestimmt der
  Kern die Abzweigung deshalb aus dem Winkel der zusammengefügten Linie am Wegpunkt (wie oben,
  20 m, unter 30° kein Hinweis). Liegt schon eine Abzweigung von Valhalla höchstens 20 m vom
  Wegpunkt entfernt, gilt diese, und keine zweite kommt dazu: So weit reicht die Messung des
  Winkels, eine so nahe Abzweigung knickt ihn mit. An einer Naht zu oder von einem
  Teilstück `Manual` oder einem gescheiterten gibt es keinen Hinweis. Am Ende jeder Etappe vor dem
  Ziel kommt „Etappenziel erreicht“; liegt es an einer Abzweigung, steht diese davor. Das Ziel
  selbst meldet die Phase `arrived`. Teilstücke aus der Zeit vor den Abzweigungen (Format 2)
  haben keine; dann ist `has_turns` falsch und die Leiste sagt einmal „Diese Route hat keine
  Abbiegehinweise“.
- **Abstände:** Die Klasse kommt vom Profil des Teilstücks, auf dem der Fortschritt liegt;
  `Manual` hat keine Abbiegehinweise; ein Etappenziel kommt dort mit den Abständen zu Fuß. Vorankündigung und „Jetzt“ kommen beim größeren Wert aus Zeit ×
  Tempo und Untergrenze in Metern. Die Untergrenzen sorgen dafür, dass der Hinweis bei einem Halt
  (Tempo 0) trotzdem kommt; beim Wandern gelten feste Abstände, das Tempo ist dort zu klein und
  zu schwankend.

  | Klasse | Profile | Vorankündigung | Unmittelbar („Jetzt“) | Tempo ohne GPS |
  |---|---|---|---|---|
  | Zu Fuß | `Hike` | 50 m | 12 m | — |
  | Rad | `RoadBike`, `Trekking`, `Mtb` | 25 s, mindestens 100 m | 6 s, mindestens 30 m | 4,5 m/s |
  | Auto | `Car` | 35 s, mindestens 400 m; ab 22 m/s (80 km/h) mindestens 1500 m | 9 s, mindestens 100 m | 14 m/s |

  Ist die Abzweigung schon näher als der Abstand für „Jetzt“ (nach dem Start oder einer
  Neuplanung), kommt gleich „Jetzt …“ ohne Vorankündigung. Jede Stufe kommt je Abzweigung
  höchstens einmal und fällt nie zurück, auch wenn GPS-Sprünge den Fortschritt vor und zurück
  werfen. Ein Etappenziel kommt nur als „Jetzt“, ohne „Jetzt“ im Text.
- **Tempo:** der Mittelwert der Fixes der letzten 5 s. Je Fix zählt das GPS-Tempo, ohne
  brauchbares (fehlend, negativ) der Fortschritt seit dem Fix davor, aber nur, wenn dieser in den
  5 s liegt: Über eine GPS-Lücke hinweg wird kein Tempo abgeleitet. Ohne jedes Tempo gilt das der
  Tabelle. Fixes mit mehr als 50 m Ungenauigkeit nimmt der Guide gar nicht an (Abschnitt 7).
- **Vorbei:** 10 m hinter einer Abzweigung gilt sie als passiert; bis dahin bleibt das Feld
  stehen, der Abstand zählt bis 0 herunter. Folgt innerhalb dieser 10 m ein Etappenziel, gilt
  die Abzweigung schon an ihrer Stelle als passiert, sobald ihr „Jetzt“ kam, damit das
  Etappenziel rechtzeitig kommt. Ebenso gilt ein Etappenziel an seiner Stelle als passiert,
  sobald es gesagt ist, damit eine Abzweigung kurz dahinter gleich danach kommt.
- **Dicht beieinander:** Liegt die nächste Abzweigung höchstens so weit hinter der ersten, wie der
  Abstand für „Jetzt“ beim aktuellen Tempo beträgt, kommen beide in einen Hinweis („Jetzt rechts,
  dann gleich links“). Die zweite bekommt dann keine eigene Vorankündigung; das Feld zeigt sie nach
  der ersten bis zu ihrem eigenen „Jetzt“. Liegt sie weniger als 10 m hinter der ersten, gilt auch
  ihr „Jetzt“ als gesagt. Ein Etappenziel kommt nicht in den Satz und wird immer für sich gesagt.
- **Nur auf der Route:** Hinweise gibt es nur in der Phase `on_route` und nur, solange der Fix
  höchstens 40 m von der Linie liegt: Von weiter weg (etwa beim Start fern der Route, bevor die
  Abweichung feststeht) ist der Fortschritt nur der nächste Punkt der Linie. Eine neue Führung
  (Start, Umplanen) sagt beim Platzieren mit dem letzten Fix noch nichts; die Apps zählen die
  Hinweise ab ihrem ersten Status neu, der erste Hinweis kommt mit dem nächsten Fix. Abseits der Route, auf dem
  Weg zurück und nach der Ankunft ist `turn` leer; der Weg zurück hat keine eigenen Hinweise. Auf
  dem Wegenetz plant Trail nach einer Abweichung gleich um (Abschnitt 7), dann gelten die Hinweise
  der neuen Route.
  Zurück auf der Route geht es bei der nächsten Abzweigung weiter, ohne schon Gesagtes zu
  wiederholen.
- **An Swift** (`core-ffi`, `TurnInfo`): `kind` ist der Serde-Name der Art (etwa `right`,
  `roundabout`, `stage_end`), `stage` `ahead` oder `now`, dazu Abstand, Abstandstext, Kurztext und
  der Satz für Mitteilung und Sprache. Texte rundet der Kern: unter 100 m auf 10 m (mindestens
  „10 m“), bis 1 km auf 50 m, ab 975 m auf 0,1 km mit Komma („1,2 km“).

## 8. Gleitzahl, Landepunkt, Wind (`flight-core`)

### Gleitzahl

Modul `glide`. Die Gleitzahl über Grund ist Strecke geteilt durch verlorene Höhe im
Gleitflug, gemessen über ein gleitendes Fenster (Einstellung 10, 20, 30 oder 60 s, Standard 20 s).

- **Puffer:** Es bleiben immer die letzten 62 s an Messpunkten, egal welches Fenster gilt;
  ein neues Fenster wirkt so sofort. Eine Lücke über 3 s zwischen zwei Punkten leert den Puffer.
- **Auswertung:** Über die Punkte im Fenster (vom neuesten aus zurück). Das Fenster muss
  mindestens `Fenster - 1 s` abdecken, sonst gibt es nichts.
- **Geradeaus:** Verschiebung `d` (Großkreis erster bis letzter Punkt) geteilt durch den
  gesamten Weg `p` (Summe der Teilstrecken) muss mindestens 0,8 sein. Kreisen liefert viel
  weniger. Es zählen Positionen, nicht der GPS-Kurs; dessen Rauschen stört nicht.
- **Tore:** Mittlere Grundgeschwindigkeit `d / Spanne` mindestens 1,4 m/s (ca. 5 km/h).
  Mittlere Sinkrate (`Höhenverlust / Spanne`) mindestens 0,1 m/s; sonst (Steigen, Nullschieber)
  gibt es nichts.
- **Ergebnis:** `Gleitzahl = d / Höhenverlust`, gedeckelt bei 40. Der Kurs ist die Peilung vom
  ersten zum letzten Punkt über Grund; er zeigt also nach hinten, wenn der Wind rückwärts
  versetzt.
- Lieber nichts anzeigen als etwas Falsches: Steigen, zu langsam, nicht geradeaus oder ein noch
  nicht volles Fenster (auch nach einer GPS-Lücke) ergeben `None`. Die Geradeaus-Prüfung allein
  erkennt Kreisen aber nicht sicher (direkt nach dem Bart, oder beim Kreisen in starkem Wind, wo
  die Spur sich kaum einrollt); das übernimmt die Engine (siehe Anzeige-Sperren).

### Landepunkt

Modul `landing`. Wo die aktuelle Gleitlinie das Gelände trifft, entlang des Kurses über Grund.

- **Schritte:** Alle 50 m (`LANDING_STEP_M`) entlang des Kurses (`geo::destination`), höchstens
  50 km weit (`LANDING_MAX_RANGE_M`). Die Höhe der Gleitlinie ist `Höhe - Strecke / Gleitzahl`;
  Höhe und Gelände sind beide über Meer (GPS-Höhe).
- **Treffer:** Der erste Schritt, an dem die Linie höchstens so hoch wie das Gelände liegt. Die
  genaue Strecke ergibt sich linear zwischen diesem und dem vorigen Schritt (Differenz Linie minus
  Gelände), die Position wird dafür neu berechnet.
- **Nichts statt Falsches:** Liegt der Start schon unter dem Gelände, ist das Gelände am Start oder
  irgendwo auf dem Weg unbekannt (außerhalb der Regionen), oder gibt es keinen Treffer innerhalb
  der Reichweite, so gibt es `None`.

### Wind

Modul `wind`. Beim Kreisen liegen die Geschwindigkeiten über Grund auf einem Kreis, dessen
Mittelpunkt der Windvektor ist (die Fluggeschwindigkeit dreht sich, der Wind bleibt).

- **Punkte:** Je GPS-Punkt `x = v·sin(Kurs)` (Ost), `y = v·cos(Kurs)` (Nord).
- **Vollkreis:** Kursänderung je Punkt (gefaltet auf −180°..180°) durch die Zeit. Ein Kreis läuft,
  solange die Drehrate mindestens 4°/s in gleicher Richtung beträgt; bis zu 2 schwächere Punkte
  in Folge sind erlaubt, der dritte bricht ab, ebenso eine Lücke über 3 s oder eine Dauer über
  60 s. Eine starke Drehung in Gegenrichtung beendet den Kreis und beginnt einen neuen in der
  neuen Richtung (sonst hielte Zickzack-Rauschen einen Kreis ewig offen). Bei 360° Summe ist der
  Kreis fertig, der nächste beginnt an diesem Punkt.
- **Anpassung:** Algebraischer Kleinste-Quadrate-Kreis (Kåsa, 3×3-Normalgleichungen). Gültig nur
  mit mindestens 8 Punkten, höchstens 60 s Dauer, Radius mindestens 3 m/s und RMS-Rest / Radius
  höchstens 0,3. Der Mittelpunkt ist der Wind (wohin); angezeigt wird „woher“.
- **Mittel:** Höchstens 6 Kreise, Gewicht 0,7^k (neuester k = 0) für Mittelpunkt und Restgüte.
  Ein Kreis, der mehr als 120 s nach dem Ende des letzten beginnt (neuer Bart), oder dessen
  Wind mehr als 2,5 m/s vom Mittel abweicht, ersetzt die Liste.
- **Sterne:** Minimum aus Anzahl (1, 2, ab 3 → 3) und Güte (≤ 0,1 → 3, ≤ 0,2 → 2, sonst 1).
  Minus 1 ab 5 min Alter (seit dem Ende des neuesten Kreises), minus 2 ab 15 min, ab 30 min
  nichts. Minus 1, solange ein Kreis in einem neuen Bart schon 180° gedreht hat (alter Wind).
  Unter 1 Stern nichts.
- **Letzte Drehung:** `last_turn_s` ist die Zeit des letzten Punkts einer echten Drehung: eines
  laufenden Kreises ab 60° Kursänderung, des Kreises nach einem fertigen Kreis oder einer starken
  Gegendrehung direkt nach einem Stück, das selbst mindestens 60° gedreht hat. Kurs-Zickzack im
  Geradeausflug kommt nie so weit, auch nicht direkt nach dem Kreisen (kleine Gegendrehungen
  geben die Drehung nicht weiter). Kreist der Schirm in Wind stärker als seine Fahrt, pendelt der
  Kurs nur hin und her (bei 8 m/s Fahrt und 12 m/s Wind um ±42°), zählt so aber durchgehend als
  Drehung.

### Anzeige-Sperren (`vario-engine`)

Die Engine (`Processor`) zeigt Gleitzahl und Landepunkt nur, wenn sie gerade etwas aussagen:

- **Geschwindigkeit:** mindestens 1,4 m/s (ca. 5 km/h) über Grund im Fenster (Modul `glide`).
- **In der Luft:** nur zwischen erkanntem Start und erkannter Landung (Abschnitt 9). Das
  ersetzt die frühere Sperre „mindestens 30 m über Gelände“.
- **3D-Fix:** Mit 2D-Fix gibt es nichts; die Position kann dann hunderte Meter daneben liegen.
- **GPS-Verlust:** Ist der letzte Fix älter als 3 s, verschwinden beide (gemessen an der Zeit der
  übrigen Sensoren, ohne jeden Sensor an der Uhr der Engine). Die Luftraumwarnungen enden erst
  nach 30 s.
- **Nach dem Kreisen:** Keine Gleitzahl, solange seit der letzten Drehung (`last_turn_s` aus
  `wind`) noch kein ganzes Fenster vergangen ist. Sonst enthielte das Fenster noch Steigen oder
  Kreisstücke und zeigte z. B. direkt nach dem Bart Werte um 40 statt 8.
- **Landepunkt:** Die Anfrage nimmt die GPS-Höhe (MSL, wie das Gelände), die Gleitzahl selbst
  die Druckhöhe des Varios (nur Differenzen zählen). Ein eigener Thread sucht den Treffer und
  rechnet nur die neueste Anfrage. Ist keine Gleitzahl sichtbar, wird auch kein Landepunkt
  gezeigt und das alte Ergebnis verworfen.

## 9. Start- und Landeerkennung (`flight-core`, Modul `flight_phase`)

Ein Zustandsautomat (bereit → in der Luft → gelandet → in der Luft …) bekommt je GPS-Fix
Zeit, Position, Geschwindigkeit über Grund, Steigwert des Varios und Höhe über Gelände
(GPS-Höhe minus `dem.pmtiles`, nur mit 3D-Fix). Er meldet Start und Landung, beide auf den
Zeitpunkt zurückdatiert, an dem sie wirklich geschahen. Vorbild ist der Flymaster GPS M; wo
das Gelände bekannt ist, entscheidet Sky über die Höhe statt über die Geschwindigkeit.

- **Start mit Gelände:** mindestens 20 m über Gelände, 5 s lang. Eine Autofahrt bleibt am
  Boden und startet keinen Flug; ein Start im starken Wind mit kaum Fahrt über Grund wird
  trotzdem erkannt.
- **Start ohne Gelände** (wie Flymaster „Start Speed“): mindestens die Startgeschwindigkeit
  (Einstellung, Standard 15 km/h), 5 s lang. Kurze Ausreißer der GPS-Geschwindigkeit starten
  nichts.
- **Zurückdatieren des Starts:** auf den letzten Fix „in Ruhe“ vor Beginn der Startbedingung:
  langsamer als 5 km/h, Steigwert innerhalb ±0,5 m/s, mit Gelände unter 20 m darüber. Der
  Steigwert gehört dazu, weil beim Start im starken Wind die Fahrt über Grund so klein ist wie
  am Boden, der Schirm aber schon steigt. Gibt es in den letzten 60 s keinen solchen Fix,
  gilt der älteste gemerkte.
- **Landung** (Flymaster „On Landing“): langsamer als 5 km/h und Steigwert innerhalb
  ±1,5 m/s, mit Gelände zusätzlich unter 20 m darüber, 30 s lang; wo die Höhe über Gelände
  unbekannt ist, 60 s lang. Die Bodennähe ist eine Ergänzung zum Flymaster: Beim Soaren im
  starken Wind (keine Fahrt über Grund, Nullschieber, hoch über dem Hang) würde dessen Regel
  sonst mitten im Flug eine Landung melden. Die Landung wird auf den ersten ruhigen Fix
  zurückdatiert.
- **Fehlende Daten beenden keinen Flug:** Eine unbekannte Geschwindigkeit ist nicht ruhig.
  Ein unbekannter Steigwert (noch kein Barometer) stört weder Ruhe noch Start. Liegen zwei Fixe
  mehr als 3 s auseinander, beginnen alle Zählungen neu, auch eine Lücke allein beendet nichts.
- **Bekannte Grenzen:** In der Seilbahn ist Sky hoch über dem Gelände; wer Sky dort schon
  bereit macht, bekommt einen kurzen Scheinflug bis zur Bergstation. Ohne Gelände startet wie
  beim Flymaster auch eine Autofahrt einen Flug. Die Höhe über Gelände schwankt mit GPS-Höhe
  und Geländemodell (Zoom 13, am Brauneck ca. 13 m je Pixel) um einige Meter.
- **Ton:** Mit „Ton erst ab Start“ (Standard an) bleiben Vario- und Warnton außerhalb eines
  Flugs stumm; Warnbanner und Hervorhebung auf der Karte bleiben. Die Testknöpfe spielen immer.
