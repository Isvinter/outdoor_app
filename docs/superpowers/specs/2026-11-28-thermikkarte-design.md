# Thermikkarte für ZeroGrid Sky

Stand: 28.11.2026. Code, Kommentare und Commits auf Englisch, sichtbare Texte auf Deutsch.

## Absicht

Der Nutzer (A-Schein, Richtung Streckenfliegen und Hike & Fly) will auf der Karte sehen, wo
erfahrungsgemäß Thermik steht. Er braucht das **bei der Vorbereitung** (ein Gelände
kennenlernen, den Flug planen) und **im Gleitflug**, um den nächsten Bart zu finden. Dass es
Karten gibt, die Jahreszeit und Tageszeit berücksichtigen, war ihm neu; genau das will er.

Wie bei den anderen Anzeigen gilt: Die Karte ist eine Statistik vergangener Flüge, keine
Vorhersage, und Sky ist kein zertifiziertes Instrument. Dem Nutzer ist ein ruhiges Display
wichtig, deshalb lässt sich die Karte im Flug mit einem Knopf ein- und ausschalten.

## Datenquelle

**kk7 Thermal Maps** (thermal.kk7.ch, M. von Känel). Wahrscheinlichkeit, auf Thermik zu
treffen, berechnet aus etwa 3,9 Mio. Streckenflügen (XContest, DHV-XC, Leonardo u. a.),
Raster etwa 100 m. Genutzt auch von XCTrack, SeeYou, XCSoar u. a.

- **Lizenz:** CC BY-NC-SA 4.0. Nicht-kommerziell heißt laut kk7: keine Bezahl-App, keine
  bezahlten Funktionen, keine Werbung. Sky ist privat und kostenlos – passt. Wird Sky je
  verkauft oder mit Werbung angeboten, braucht es eine eigene Vereinbarung mit kk7.
- **Nennung:** „thermal.kk7.ch CC-BY-NC-SA“ (Vorlage von kk7).
- **Regeln für fremde Apps:** Kacheln nur bei Bedarf für Ausschnitt und Zoom holen, den der
  Nutzer ansieht; **kein Vorabladen**, besonders nicht ganzer Regionen, Zoombereiche oder
  Layer. **Zwischenspeichern auf dem Gerät ist erlaubt**, Lebensdauer 30 Tage. Jede Anfrage
  trägt `?src=<App>`, eigene Clients nennen sich im User-Agent, höchstens 10 Anfragen je
  Sekunde.
- **Kacheln:** 256-px-PNG im TMS-Schema (y gespiegelt), Zoom 0–12:
  `https://thermal.kk7.ch/tiles/thermals_<key>/{z}/{x}/{y_tms}.png?src=<App>` mit
  `y_tms = 2^z − 1 − y`.
- **Layer-Schlüssel** `<jahreszeit>_<tageszeit>`, gelesen aus kk7s eigenem Seiten-Code
  (`js/main.js`), nicht dokumentiert:
  - Jahreszeit: `all` (ganzes Jahr), `jan` (Mitte Nov.–Mitte März), `apr` (Mitte
    Feb.–Mitte Juni), `jul` (Mitte Mai–Mitte Sept.), `oct` (Mitte Aug.–Mitte Dez.).
  - Tageszeit: `all` (ganzer Tag), `04` (morgens: Sonnenaufgang bis +7 h), `07` (mittags:
    +5 bis +11 h), `10` (abends: ab +7 h). Die Zeiträume überlappen.
  - Also 20 Layer, z. B. `thermals_jul_07`, `thermals_all_all`.

Andere Quellen wurden geprüft und verworfen: TherMap (alt, wenige Regionen), GLIDAR
(Wettkampfdaten), Forschungsdatensatz Frankreich (eine Region, Lizenz unklar), WeGlide
(kommerziell). Eigene Thermikpunkte aus Skys Kreiserkennung wären lizenzfrei, haben aber
anfangs zu wenig Daten – spätere Idee.

## Ziele

1. Sky zeigt die kk7-Thermikkarte halb durchsichtig auf der Karte.
2. Ein **Knopf „Thermik“ auf der Karte** (neben „Folgen“) schaltet sie ein und aus. Der
   Zustand bleibt über Neustarts erhalten; voreingestellt aus.
3. Sky wählt den Layer **automatisch** nach Jahreszeit und Tageszeit. In den Einstellungen
   lassen sich beide auch fest wählen (z. B. abends „morgen mittags“ anschauen).
4. Was angesehen wurde, liegt im Speicher auf dem Gerät und steht im Flug **ohne Netz** zur
   Verfügung. Mit Netz (Mobilfunk im Flug) lädt Sky fehlende Kacheln nach.
5. Die Nennung steht sichtbar in der Karte, solange die Thermikkarte an ist, und unter
   Datenquellen.

## Nicht-Ziele

- Keine Hotspot-Punkte (kk7 bietet sie als GeoJSON je Layer an:
  `api/hotspots/geojson/<key>/<süd>,<west>,<nord>,<ost>?limit=250`). Das ist der nächste
  Schritt („B“), eigenes Vorhaben.
- Kein Knopf „Ausschnitt laden“ und kein Vorabladen einer Region – verbietet kk7, und der
  Nutzer will es nicht (Entscheidung unten).
- Keine Skyways-, Spotlight- oder 3D-Thermik-Layer.
- Keine eigenen Thermikpunkte aus Skys Flügen.
- Kein Weg über Base (Base lädt nichts von kk7 und verteilt nichts davon) – das wäre
  Weitergabe und Vorabladen zugleich.
- Keine Verbindung mit Gleitzahl/Landepunkt („nächster Bart in Reichweite“) – spätere Idee.

## Was auf keinen Fall passieren darf

- **Eine Anfrage an kk7, während die Thermikkarte aus ist.** Aus heißt: keine Quelle, kein
  Netz.
- Sky lädt Kacheln, die der Nutzer nicht ansieht (Nachbarkacheln, andere Zoomstufen, andere
  Layer „auf Vorrat“).
- Ein langsames oder fehlendes Netz lässt die Karte stocken, hält die Kacheln der
  Offline-Regionen auf oder bremst Ton, Aufzeichnung oder Luftraumwarnung.
- Die Thermikkarte verdeckt Lufträume, Route, Spur, Landepunkt oder das Warnbanner.
- Persönliche Daten gehen an kk7 (Position steckt zwangsläufig in den Kachelnummern; mehr
  nicht – kein Name, keine Kennung des Geräts).

## Entscheidungen des Nutzers

- **Weg A:** Flächenkarte von kk7 mit Zwischenspeicher. Hotspot-Punkte (B) kommen später.
- **Knopf auf der Karte** zum Ein- und Ausschalten, nicht nur in den Einstellungen.
- **Automatisch laden, nur bei eingeschalteter Thermikkarte.** Auf die Frage, ob beim
  Herauszoomen auf ganz Europa alles geladen wird: nein, nur die wenigen groben Kacheln, die
  den Bildschirm füllen. Damit brauchte es keinen Knopf „Ausschnitt aktualisieren“ und
  keinen Schalter „nur aus dem Speicher“.
- **Fehlt offline die passende Tageszeit, nimmt Sky eine andere gespeicherte derselben
  Jahreszeit** (Hausbärte stehen morgens und mittags meist an ähnlichen Stellen). Dazu ein
  Hinweis zur Vorbereitung, die Tageszeit von Hand auf „Mittags“ zu stellen. Kein
  automatischer Wechsel der Tageszeit am Boden.
- **Gespeicherte Kacheln werden nie von selbst gelöscht.** So wächst über die Jahre ein
  eigener Bestand für alle Jahreszeiten und Tageszeiten – für die Gebiete, die man je mit
  eingeschalteter Thermikkarte angesehen hat. Mit Netz fragt Sky bei Kacheln älter als 30
  Tage zuerst online nach; ohne Netz nimmt es die alte Kachel, egal wie alt. Idee des
  Nutzers: Thermikstellen ändern sich von Jahr zu Jahr kaum.
- **App-Kennung `ZeroGrid`** gegenüber kk7 (nicht die Bundle-ID, die den Nachnamen
  enthält).

## Verhalten

### Knopf auf der Karte

- „Thermik“, im Stil von „Folgen“ (`map-overlay`, Klasse `on` wenn an).
- An: Quelle und Layer werden hinzugefügt, Kacheln werden geholt. Aus: Layer und Quelle
  werden entfernt (nicht nur unsichtbar gestellt), damit MapLibre nichts mehr anfragt.
- Der Zustand überlebt Stilwechsel (hell/dunkel, Relief) – Sky fügt Quellen nach jedem
  Stilwechsel neu hinzu (`map.js`, wie bei Lufträumen und Landepunkt).

### Layer-Wahl

- **Jahreszeit automatisch** aus dem Datum (Gerätezeit), nächste Monatsmitte:
  Dez.–Feb. → `jan`, März–Mai → `apr`, Juni–Aug. → `jul`, Sept.–Nov. → `oct`.
  kk7 rechnet in Kalendermonaten; Südhalbkugel (Cusco) braucht deshalb keine Sonderregel.
- **Tageszeit automatisch** aus den Stunden seit Sonnenaufgang am aktuellen Ort (GPS, sonst
  Kartenmitte): unter 5,5 h → `04`, 5,5 bis unter 8,5 h → `07`, ab 8,5 h → `10` (Mitten
  zwischen den kk7-Zeiträumen). Vor Sonnenaufgang → `04`. Polartag/-nacht ohne
  Sonnenaufgang → `all`.
- Sky prüft die Wahl etwa einmal pro Minute; ändert sie sich (Tageszeit wechselt im Flug),
  bekommt die Quelle die neuen Kachel-Adressen.
- **Manuelle Wahl in den Einstellungen**, Abschnitt „Thermikkarte“:
  - Jahreszeit: Automatisch · Ganzes Jahr · Winter (Jan.) · Frühling (Apr.) · Sommer (Juli)
    · Herbst (Okt.) – je mit dem Zeitraum aus kk7 als Erklärung.
  - Tageszeit: Automatisch · Ganzer Tag · Morgens · Mittags · Abends – ebenso.
  - Voreingestellt beide „Automatisch“. Die Wahl bleibt gespeichert.

### Laden, Speichern und Offline

- Die Karte fragt `tiles://localhost/thermals/<key>/<z>/<x>/<y>` an (XYZ). Die Rust-Seite
  (`maps.rs`) beantwortet das, wie heute die Regionskacheln.
- Reihenfolge der Antwort:
  1. Kachel im Speicher und jünger als 30 Tage → ausliefern.
  2. Sonst bei kk7 holen (kurzer Timeout, siehe Builder) → ablegen → ausliefern.
     kk7 antwortet für Gebiete ohne Daten mit 200 und einer leeren, transparenten PNG
     (68 Byte, geprüft am 28.11.2026) – wie jede andere Kachel speichern.
  3. Holen scheitert (kein Netz, Fehler) → ältere Kachel desselben Layers, falls
     vorhanden, auch wenn älter als 30 Tage.
  4. Sonst **Ersatz-Layer** aus dem Speicher, in dieser Reihenfolge: andere Tageszeiten
     derselben Jahreszeit (nächstliegende zuerst: von `04` aus `07`, dann `10`; von `10`
     aus `07`, dann `04`; von `07` aus `04`, dann `10`), dann `<jahreszeit>_all`, dann
     `all_all`. Bei manuell gewähltem `all`-Wert ebenso sinngemäß.
     Ersatz-Antworten werden vom Web-View nicht gespeichert (`Cache-Control: no-store`),
     damit später mit Netz die richtige Kachel kommt.
  5. Sonst 204 (nichts zeichnen).
- **Quelle mit `maxzoom` 10.** MapLibre fragt ab Zoom 10 immer die Kacheln der Stufe 10 an
  und vergrößert sie selbst. Eine Kachel der Stufe 10 misst in den Alpen etwa 27 km, ein
  Pixel etwa 100 m – das ist ungefähr die Auflösung der kk7-Daten, also kaum Verlust.
  Gewinn: **Zur Vorbereitung reicht es, das Fluggebiet einmal bei Zoom 10 oder näher
  anzusehen** (ein, zwei iPhone-Bildschirme pro Gebiet); im Flug bei Zoom 13–15 liegen dann
  alle nötigen Kacheln vor. Unter Zoom 10 werden die gröberen Stufen für die Übersicht
  geholt.
- **Gedrosselt** auf höchstens 10 Anfragen je Sekunde an kk7; doppelte Anfragen nach
  derselben Kachel, während eine läuft, werden zusammengelegt.
- Anfragen tragen `?src=ZeroGrid` und den User-Agent `ZeroGrid/<Version> (Sky)`; keine
  weiteren Kennungen. Eine Anmeldung bei kk7 ist dafür nicht nötig.
- Ablage: im Cache-Ordner der App, `thermals/<key>/<z>/<x>/<y>.png`. Eine Datei pro
  Kachel; das Dateidatum ist das Abrufdatum. **Nichts wird automatisch gelöscht**; nur der
  Knopf in den Einstellungen leert den Speicher. Der Cache-Ordner der App kann auf iOS bei
  Speichermangel vom System geleert werden – der Builder prüft, ob ein Ordner unter
  Application Support (vom Backup ausgenommen) besser passt, damit der Bestand bleibt.

### Darstellung

- Rasterlayer, Deckkraft etwa 0,6, über Relief und Höhenlinien, unter Lufträumen, Route,
  Spur, Landepunkt und Beschriftungen (wo das im Stil nicht geht: unter Lufträumen, Route,
  Spur und Landepunkt ist Pflicht). Farben wie von kk7 geliefert.
- Gleich in hellem und dunklem (OLED-)Stil; ob die Deckkraft im dunklen Stil anders sein
  muss, entscheidet der Builder nach Augenschein.
- Nennung als `attribution` der Quelle: „Thermik © thermal.kk7.ch (CC BY-NC-SA 4.0)“ –
  MapLibre zeigt sie dann nur, solange die Quelle da ist.

### Einstellungen

- Abschnitt „Thermikkarte“ mit den beiden Auswahlen, einer kurzen Erklärung (Statistik
  vergangener Streckenflüge, keine Vorhersage; geladen wird nur, was man bei eingeschalteter
  Thermikkarte ansieht; zur Vorbereitung Tageszeit auf „Mittags“ stellen und das Gebiet bei
  Zoom 10 oder näher ansehen, danach wieder „Automatisch“), der
  Größe des Speichers und einem Knopf „Thermik-Speicher leeren“.
- Nennung und Lizenz auch unter „Datenquellen“ und in `docs/DATENQUELLEN.md`.

## Ansatz und Alternativen

| Weg | Was | Abwägung |
|---|---|---|
| **A – Flächenkarte mit Zwischenspeicher (gewählt)** | kk7-Kacheln über die bestehende `tiles://`-Auslieferung, auf dem Gerät gespeichert | Einziger Weg mit Jahreszeit und Tageszeit als Fläche; offline nur, was vorher angesehen wurde |
| B – Hotspot-Punkte importieren | GeoJSON/CSV der Punkte für einen Ausschnitt, wie die Lufträume | Sicher offline, im Gleitflug klar lesbar; Punkte statt Fläche. Kommt später |
| C – A und B zusammen | beides | Summe des Aufwands; B lässt sich später ohne Umbau ergänzen |
| Kacheln direkt im Web-View laden | MapLibre holt die URL von kk7 selbst | Kein eigener Speicher, kein Ersatz-Layer, kein User-Agent – offline unbrauchbar |
| Region in Base vorab laden | Base holt alle Kacheln einer Region | Von kk7 verboten |

## Was sonst berührt wird

- `apps/sky/src-tauri/src/maps.rs` (Auslieferung), neues Modul für Speicher und Abruf
  (`ureq` mit `rustls` ist schon da), Einstellungen (`settings.rs` oder eigene Datei wie
  `map-look`), `lib.rs` (Befehle).
- `apps/sky/ui/map.js`, `app.js`, `index.html`, `style.css`.
- Sonnenaufgang: kleine, testbare Funktion (z. B. in `flight-core`), es gibt noch keine.
- `docs/DATENQUELLEN.md`, ggf. `docs/ALGORITHMEN.md` (Layer-Wahl, Sonnenaufgang).

**Merge-Konflikte:** Gleitzahl/Landepunkt/Wind (PR #9) und Start-/Landeerkennung sind in
main; auf GitHub ist kein offener Sky-Branch. Die Arbeit beginnt auf einem frischen
Feature-Branch von main. Hat der Nutzer auf dem Mac noch nicht eingecheckte Änderungen,
treffen sie am ehesten `map.js`, `app.js` und `index.html`.

## Tests

- Rust: Layer-Wahl (Monatsgrenzen, Stundengrenzen, Polartag), Sonnenaufgang gegen bekannte
  Werte, TMS-Umrechnung, Antwortreihenfolge mit Speicher und Ersatz-Layer mit einem
  austauschbaren Abruf (kein Netz in Tests), Drossel.
- Der Rauchtest (`smoke.rs`) und alle Tests laufen ohne Netz und ohne Anfragen an kk7.
- UI-Selbsttest mit echten Klicks: Knopf an → Quelle da; Knopf aus → Quelle weg und keine
  weiteren Anfragen an `tiles://localhost/thermals/`.
- Von Hand auf Mac und iPhone: Gebiet online ansehen, Flugmodus, Gebiet bei Zoom 13–15
  wieder ansehen; dann im Flugmodus eine andere Tageszeit wählen → Ersatz-Layer erscheint.

## Dem Builder überlassen

- Timeout und Wiederholungen beim Abruf (Richtwert: wenige Sekunden, damit die Karte nicht
  hängt; fehlgeschlagene Kacheln nicht in schneller Folge erneut versuchen).
- Wie die Drossel gebaut wird; ob Abrufe im Thread-Pool der Kachel-Auslieferung laufen oder
  in eigenem.
- Sonnenaufgangs-Formel (NOAA-Näherung genügt, ±2 min reichen).
- Genaue Lage des Knopfs neben „Folgen“, Deckkraft im Detail.
- Ablageort auf iOS (siehe oben). Größe: eine Kachel 0,1–30 KB; ein Gebiet bei Zoom 10
  mit allen 20 Layern bleibt im Bereich weniger MB.

## Nachtrag 28.11.2026: Deckkraft-Regler und Selbsttest

Nach dem ersten Ausprobieren auf dem Desktop (Nutzer): Im hellen Modus passt die Deckkraft,
im dunklen ist sie zu stark.

- **Regler „Deckkraft“** im Abschnitt „Thermikkarte“ der Einstellungen, 10–100 % in
  5er-Schritten, wirkt sofort auf die Karte.
- **Ein Wert je Modus** (Entscheidung des Nutzers): hell und dunkel getrennt gespeichert; der
  Regler zeigt und ändert den Wert des aktuellen Modus. Startwerte hell 60 %, dunkel 35 %.
  Gespeichert in `thermal-map.json` (`opacityLight`, `opacityDark`, ganze Prozent); fehlende
  oder unsinnige Werte → Startwert bzw. auf 10–100 begrenzt.
- **Selbsttest** prüft, dass eine Thermik-Kachel wirklich auf der Karte ankommt (der
  Fehler mit dem fehlenden `Access-Control-Allow-Origin` blieb sonst unbemerkt). Dazu
  liefert der Selbsttest ohne `VARIO_SMOKE_KK7` eine eingebaute Testkachel statt kk7 und
  nutzt einen eigenen Speicherordner im temporären Verzeichnis, damit der echte
  Thermik-Speicher unberührt bleibt. Er meldet auch die angewandte Deckkraft.
