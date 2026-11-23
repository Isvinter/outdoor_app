# OLED-Dunkelmodus für ZeroGrid Trail und Sky

Stand: 23.11.2026. Code, Kommentare und Commits auf Englisch, sichtbare Texte auf Deutsch.

## Absicht

Auf mehrtägigen Touren ohne Steckdose zählt jedes Prozent Akku. Auf OLED-Displays verbraucht
ein schwarzer Pixel praktisch keinen Strom, und die Karte füllt fast den ganzen Bildschirm.
Trail und Sky bekommen deshalb eine dunkle Darstellung, die möglichst wenig leuchtet und
trotzdem gut ablesbar bleibt. Das helle Design bleibt in beiden Apps wählbar, weil es in
praller Sonne die bessere Wahl sein kann.

## Ziele

- Karte und Bedienelemente lassen sich in Trail und Sky zwischen **Hell** und **Dunkel**
  umschalten. Die App merkt sich die Wahl.
- Die dunkle Karte hat echt schwarzen Grund. Wege, Gewässer, Namen und Hindernisse bleiben
  gut lesbar.
- Das Relief ist zuschaltbar: in Trail als stufenloser Regler, in Sky als Schalter An/Aus.
- Dunkle Varianten gibt es für Sommer und Winter (Trail) sowie Flug und Topo (Sky).
- Die Hangneigungsfarben der Winterkarte behalten ihre Bedeutung. Der vorhandene Regler
  bestimmt, wie stark sie leuchten.

## Nicht-Ziele

- Keine Überarbeitung der Oberfläche von Sky (eigene Session) und keine neue Struktur in Trail.
- ZeroGrid Base bleibt hell.
- Kein automatisches Umschalten nach iPhone-Einstellung, Uhrzeit oder Helligkeit.
- Kein eigener Stil „dunkle Topo“ (Variante B): Der Relief-Regler deckt ihn ab.
- Online-Kartenstile (URL in Sky) werden nicht umgefärbt.

## Was auf keinen Fall passieren darf

- Eine Warnung, ein Luftraum, ein Hindernis oder eine Hangneigungsklasse ist im Dunkelmodus
  schlechter zu erkennen oder ändert die Farbbedeutung.
- Eine neue Kartenebene erscheint unbemerkt als helle Fläche.
- Das Umschalten verliert Kartenausschnitt, geplante Route, Flugspur oder Lufträume.
- Eine laufende Aufzeichnung in Sky wird durch das Umschalten gestört.

## Ansätze

1. **Drei eigene dunkle Stildateien von Hand.** Einfach und voll kontrollierbar, aber jede
   Kartenänderung muss doppelt gepflegt werden.
2. **Ein Skript erzeugt dunkle Dateien.** Weniger Handarbeit, aber weiter generierte Kopien,
   die synchron bleiben müssen.
3. **`core-maps` färbt die hellen Stile zur Laufzeit um (gewählt vom Nutzer).** Beide Apps
   nutzen denselben Code, neue Ebenen fallen im Test auf, und es gibt keine Kopien.

## Bedienung

```
Trail   [ Sommer | Winter ]   [ Hell | Dunkel ]
        Relief  ──●────────   (nur bei Dunkel)

Sky     [ Flug | Topo ]       [ Hell | Dunkel ]
        Relief  [ Aus | An ]  (nur bei Dunkel)
```

- Trail: in der vorhandenen Leiste über der Karte.
- Sky: in den Einstellungen unter Karte → Offline-Karten, neben der Stilwahl. Eine Schnellwahl
  auf der Flugkarte gehört zur späteren Überarbeitung.
- Hell/Dunkel schaltet Karte und Bedienelemente gemeinsam um.

**Vorgaben beim ersten Start** (Entscheidung des Entwurfs): Hell, Relief aus. Die Wahl bleibt
über Neustarts erhalten, in Trail in `UserDefaults`, in Sky als Datei im Konfigurationsordner
wie die vorhandene Stilwahl.

## Technik

### `core-maps`

- `StyleOptions` bekommt `appearance: Appearance` mit `Light` und `Dark`. `Light` lässt die
  Ausgabe unverändert. Base füllt seine Stile in `apps/base/ui/src/map.ts` selbst und ist
  deshalb nicht betroffen. Die vorhandenen Aufrufer und Tests in `core-ffi`, `core-maps` und
  Sky werden angepasst.
- Das Umfärben läuft in `offline_style`, **bevor** die Variante ohne installierte Region
  zurückkehrt, damit auch die leere Karte schwarz ist.
- Eine neue Datei `crates/core-maps/src/dark.rs` enthält die Palette: eine Tabelle
  `heller Farbwert → dunkler Farbwert` und Ausnahmen je Ebene. Bewusst gewählte Farben, keine
  mathematische Invertierung.
- Das Umfärben läuft rekursiv durch alle Farbwerte in `paint`, auch in Ausdrücken wie `match`
  und `interpolate`. Die drei Stile nutzen heute 103 verschiedene Farben, darunter
  `rgba(...)`-Werte für Halos, die ebenfalls abgedeckt werden. Eine unbekannte Farbe ist ein
  `MapsError::Style`.
- Flächen ohne Lesewert (Wald, Wiese, Siedlung und Ähnliches) werden schwarz oder
  ausgeblendet. Wasser und Gletscher bleiben als sehr dunkle Tönung erkennbar.
- Halos von Beschriftungen werden schwarz.
- Die Raster-Ebene `slope` bleibt unberührt, ihre Farben stecken in den Kacheln.
- Hillshade im Dunkelmodus: Schatten und Akzent schwarz, Licht als gedämpftes Grau.
- Die Ebenen `hillshade`, `contour-minor`, `contour-major` und `contour-label` gehören zum
  **Relief**. Im Dunkelmodus starten sie ausgeblendet (`visibility: none`), sodass ohne Relief
  nichts für sie gezeichnet wird.

### `core-ffi`

`offline_style` bekommt den Parameter `dark: bool`. Die Swift-Bindings werden neu erzeugt.

### Trail

- `MapStyle.url` nimmt das Erscheinungsbild an. Der Cache-Dateiname enthält es.
- `MapController` hält `appearance` und `reliefStrength` (0…1) und setzt beides nach jedem
  Laden eines Stils erneut, wie heute `slopeOpacity`.
- Relief 0: Relief-Ebenen ausgeblendet. Über 0: eingeblendet, Hillshade-Stärke und Deckkraft
  der Höhenlinien folgen dem Regler live, ohne den Stil neu zu laden.
- `.preferredColorScheme` folgt der Wahl.
- Die Panels mit `.regularMaterial` zeigen im Dunkelmodus eine schwarze Fläche statt der
  Unschärfe, im hellen Modus bleiben sie wie heute.
- Routen, Wegpunkte und Trennlinien bekommen dunkle Varianten mit gleicher Bedeutung.

### Sky

- Neuer Befehl zum Lesen und Setzen von Erscheinungsbild und Relief, gespeichert im
  Konfigurationsordner.
- `maps.rs` reicht das Erscheinungsbild an `offline_style` weiter.
- `style.css`: ein Block `[data-theme="dark"]` überschreibt die Variablen (`--ink`, `--paper`,
  `--muted`, `--line`, `--climb`, `--sink`). `color-scheme` folgt mit. Fest eingetragene
  Farben werden wo nötig auf Variablen umgestellt.
- `map.js`: Flugspur-Rand und andere helle Farben richten sich nach dem Thema. Lufträume und
  Warnungen behalten ihre Farbbedeutung, ihre Helligkeit darf für den Kontrast auf Schwarz
  angepasst werden.
- Relief an/aus setzt die Sichtbarkeit live, ohne den Stil neu zu laden.
- Nach jedem Stilwechsel kommen Lufträume, Flugspur und Warnungen wieder auf die Karte.
- Bei einem Online-Stil wird nur die Oberfläche dunkel.

## Ablesbarkeit

Vorgaben des Entwurfs, gemessen gegen Schwarz:

- Hauptbeschriftungen, Instrumentenwerte und Warnungen: Kontrast mindestens **7:1**.
- Wege, Gewässernamen und Nebenbeschriftungen: mindestens **4,5:1**.
- Hindernisse und Lufträume bleiben im Dunkelmodus mindestens so auffällig wie die Wege.
- Die Hangneigungsklassen müssen beim Standardwert des Reglers klar unterscheidbar sein.
  Reicht `0.6` auf Schwarz nicht, bekommt der Dunkelmodus einen eigenen Standardwert.

## Prüfung

**Automatisch unter Linux (`core-maps`):**

- Für alle drei Stile: Hintergrund `#000000`, jede Farbe hat eine dunkle Entsprechung, keine
  große Fläche bleibt hell.
- `Light` erzeugt exakt die bisherige Ausgabe.
- `slope` ist unverändert, und die Relief-Ebenen sind im Dunkelmodus ausgeblendet.
- Auch der Stil ohne installierte Region ist schwarz.
- Die Kontrastvorgaben werden für die Textfarben der Palette geprüft.

**Sichtprüfung:** Bilder der dunklen Karten aus echten Regionen, mit und ohne Relief, in
Alpen und Flachland. Zum Vergleich wird die mittlere Pixelhelligkeit gegenüber der hellen
Karte angegeben. Sie ist nur ein Näherungswert für den Stromverbrauch.

**Auf dem iPhone, durch den Nutzer:** Umschalten in beiden Apps, Ablesbarkeit draußen,
Winterkarte, Lufträume und Warnungen, Umschalten während einer Aufzeichnung, flüssiges
Zoomen und Verschieben mit Relief an und aus.

## Dem Umsetzenden überlassen

- Die konkreten Farbwerte der Palette innerhalb der Vorgaben.
- Die genaue Abbildung des Reglers auf Hillshade-Stärke und Deckkraft der Höhenlinien.
- Der Zuschnitt der Umschalter innerhalb der vorhandenen Leiste und Einstellungen.
