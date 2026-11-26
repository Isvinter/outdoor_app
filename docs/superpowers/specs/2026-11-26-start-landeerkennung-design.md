# Start- und Landeerkennung für ZeroGrid Sky

Stand: 26.11.2026, vom Nutzer freigegeben. Code, Kommentare und Commits auf Englisch, sichtbare
Texte auf Deutsch.

## Absicht

Der Nutzer fliegt mit Sky auf dem iPhone und dem Flymaster GPS M, allein oder beide zugleich
als Redundanz. Der Wechsel zwischen den Geräten soll ohne Umdenken gehen. Beim Flymaster
ist der **Flugstart** das Ereignis, an dem fast alles hängt: Erst ab dem Start piept das
Vario („Auto Silent“), wird der Track gespeichert und gibt es einen Startpunkt. Sky soll
denselben Begriff vom Flug haben und sich an denselben Stellen gleich verhalten.

Wo das iPhone mehr kann (Geländemodell der Regionen, Karte, größerer Bildschirm), darf Sky
es besser machen, solange das Verhalten für den Piloten gleich zu lesen bleibt.

Heute ist in Sky jeder Lauf von „Start“ bis „Stopp“ ein Flug. Die Sperren für Gleitzahl und
Landepunkt (ab 5 km/h, ab 30 m über Gelände) waren ausdrücklich als Ersatz gedacht, bis eine
echte Start-/Landeerkennung kommt (Entwurf vom 25.11.2026).

## Wie der Flymaster es macht (Handbuch, `references/`)

- **Flugstart:** sobald die GPS-Geschwindigkeit die eingestellte „Start Speed“ (km/h)
  erreicht. Mit 0 beginnt der Flug sofort beim ersten GPS-Fix.
- **Auto Silent** (Standard an): Das Vario bleibt bis zum Flugstart stumm.
- **Track:** wird erst ab dem Flugstart gespeichert. Flüge sind in der Liste nach
  Startdatum, Startzeit und Flugdauer benannt.
- **Startpunkt:** der Ort des Flugstarts. Darauf beruhen Höhe über Start, Entfernung zum
  Start und die Gleitzahl, die nötig wäre, um zum Start zurückzukommen.
- **Landung** („On Landing“, zum Abschalten): unter 5 km/h und Vario zwischen −1,5 und
  +1,5 m/s, länger als 30 s.

## Ziele

1. Sky erkennt **Start** und **Landung** selbst. Der Knopf „Start“ macht das Vario nur
   noch bereit (Sensoren, Engine, Ton), der eigentliche Flug beginnt mit dem erkannten
   Start.
2. Ein **Flug** in der Flugliste geht vom erkannten Start bis zur erkannten Landung, mit
   Startzeit, Flugdauer und Startpunkt, wie beim Flymaster.
3. **Ton erst ab Start** (wie „Auto Silent“), abschaltbar.
4. Gleitzahl und Landepunkt erscheinen nur **in der Luft**. Das ersetzt die bisherige
   Sperre über dem Gelände.
5. Die Anzeige sagt jederzeit, in welchem Zustand Sky ist: bereit, in der Luft, gelandet.

## Nicht-Ziele

- Keine neuen Werte aus dem Startpunkt (Höhe über Start, Entfernung zum Start, Gleitzahl
  zum Start). Sky merkt sich den Startpunkt, angezeigt wird er erst mit einem späteren
  Feature (Navigation oder Thermikassistent).
- Kein automatisches Beenden der App oder der Sensoren nach der Landung.
- Kein Name des Startplatzes aus dem Flug-Layer der Region in der Flugliste: spätere Idee.
- Keine Erkennung von Seilbahn- oder Autofahrt als eigener Zustand (siehe Grenzen).
- Kein Ändern bereits gespeicherter Flüge.

## Was auf keinen Fall passieren darf

- **Ein Flug geht verloren**, weil der Start nicht erkannt wurde. Was zwischen „Start“
  und „Stopp“ aufgezeichnet ist, muss am Ende als IGC-Datei da sein.
- **Eine Landung wird mitten im Flug erkannt**, zum Beispiel beim Soaren im starken Wind
  mit fast null Fahrt über Grund und Nullschieber. Am Brauneck-Sattel kam genau das schon
  vor (Böen über 30 km/h, kaum Fahrt über Grund). Beim Flymaster würde die Regel „unter
  5 km/h, ±1,5 m/s, 30 s“ hier greifen.
- Der Vario-Ton bleibt in der Luft stumm, weil der Start nicht erkannt wurde.
- Die Erkennung bremst Ton, Aufzeichnung oder Luftraumwarnung.

## Verhalten

### Zustände

```
  [Start gedrückt]
        │
        ▼
     BEREIT ──── Start erkannt ────▶ IN DER LUFT ──── Landung erkannt ────▶ GELANDET
        ▲                                                                       │
        └──────────────────────────── (wartet auf den nächsten Start) ◀────────┘
  [Stopp] beendet aus jedem Zustand
```

- **Bereit:** Sensoren und Engine laufen, Luftraumdaten und Gelände sind geladen. Die
  Aufzeichnung puffert die letzten Sekunden im Speicher (siehe Aufzeichnung). Mit
  „Ton erst ab Start“ ist das Vario stumm.
- **In der Luft:** wie heute nach „Start“: Ton, Aufzeichnung in die IGC-Datei,
  Gleitzahl, Landepunkt, Wind.
- **Gelandet:** Die Flugdatei ist fertig. Sky bleibt bereit für den nächsten Start
  (Toplanden und neu starten, Hike & Fly). Ton wieder stumm, wenn „Ton erst ab Start“ an
  ist.
- **Stopp** beendet wie heute die ganze Sitzung.

### Starterkennung

Zwei Wege, je nachdem ob das Gelände bekannt ist:

**Mit Gelände** (eine Region ist installiert, der Normalfall): Start, sobald Sky
**deutlich über dem Gelände** ist, um **20 m** für einige Sekunden. Die Höhe über Gelände
kommt aus der Höhe von Sky gegen `dem.pmtiles`.

- Im Auto bleibt Sky am Boden, es gibt also keinen Fehlstart. Der Flymaster würde hier bei
  jeder Fahrt über der Startgeschwindigkeit einen Flug beginnen.
- Beim Start im starken Wind gibt es kaum Fahrt über Grund. Die Geschwindigkeitsregel des
  Flymaster würde dann erst spät oder gar nicht greifen, die Höhe über Gelände schon.

**Ohne Gelände** (keine Region an diesem Ort): wie beim Flymaster, Start ab der
**Startgeschwindigkeit** über einige Sekunden. Voreinstellung **15 km/h**, einstellbar.

**Zurückdatieren:** Bis die Schwelle erreicht ist, ist der Pilot schon einige Sekunden in
der Luft. Sky setzt den Startzeitpunkt und den Startpunkt daher auf den Beginn der
Bewegung: den letzten Moment davor, in dem Sky noch ruhig am Boden stand. Der Flug enthält
den Startlauf.

### Landeerkennung

Grundregel wie beim Flymaster: **unter 5 km/h** über Grund und Vario zwischen **−1,5 und
+1,5 m/s**, länger als **30 s**. Dazu kommt ein Schutz gegen das Soaren im starken Wind:

- **Mit Gelände:** zusätzlich **nahe am Boden** (unter etwa 20 m über Gelände). Hoch über
  dem Hang mit null Fahrt über Grund gilt also nicht als gelandet.
- **Ohne Gelände:** dieselbe Regel, aber mit längerer Wartezeit (etwa **60 s**). Damit
  muss der Nullschieber über eine ganze Minute halten.

Die Landung wird auf den Beginn dieser ruhigen Phase zurückdatiert. Der Flug endet dort,
nicht 30 s später.

### Aufzeichnung

- **Vor dem Start** hält Sky die letzten **60 s** Messwerte im Speicher. Beim erkannten
  Start beginnt die IGC-Datei mit diesem Puffer ab dem zurückdatierten Startzeitpunkt.
- **In der Luft** wie heute: alle 10 s wird die Datei vollständig neu geschrieben.
- **Bei der Landung** wird die Datei bis zum Landezeitpunkt geschrieben und
  abgeschlossen. Ein weiterer Start ergibt eine neue Datei.
- **Kein Start erkannt** (die Erkennung hat versagt oder der Pilot hat abgebaut): Bei
  „Stopp“ wird die Sitzung wie heute als IGC-Datei gespeichert und in der Flugliste als
  **„ohne erkannten Start“** markiert. So geht kein Flug verloren. Ohne nennenswerte
  Bewegung (z. B. nur am Startplatz gestanden) wird nichts gespeichert. Die Sensorlogs
  bleiben in beiden Fällen wie heute.
- **Absturz der App in der Luft:** Die Datei bis zum letzten Speichern bleibt erhalten, der
  Hinweis auf die unterbrochene Sitzung bleibt wie heute.

### Ton

- Neue Einstellung **„Ton erst ab Start“**, voreingestellt **an** (wie der Flymaster).
- Bei „bereit“ und „gelandet“ ist das Vario dann stumm. Die Testknöpfe für Vario- und
  Warnton funktionieren weiter.
- Luftraumwarnungen: Banner und Hervorhebung immer, der **Warnton** folgt derselben
  Einstellung. Am Startplatz in der Nähe einer CTR soll nichts dauernd piepen.

### Anzeige

- **Instrumente-Seite:** Die Zeile „● Aufzeichnung: 0:22 min“ zeigt den Zustand:
  - „Bereit · wartet auf Start“
  - „● In der Luft · 0:22 min“ (Flugzeit seit dem Start)
  - „Gelandet · Flugzeit 1:12 h“
  - wie heute „Aufzeichnung: warte auf GPS“ bzw. die Fehlermeldung beim Speichern.
- **Karte:** Die Box oben links bekommt keinen weiteren Inhalt. Zu sehen ist der Zustand
  nur auf der Instrumente-Seite und am Start-/Stopp-Knopf, damit die Karte ruhig bleibt.
- **Flugliste:** Startdatum, Startzeit und Flugdauer wie bisher, aber vom erkannten
  Start bis zur Landung. Flüge „ohne erkannten Start“ sind gekennzeichnet.
- **Einstellungen:** „Ton erst ab Start“ (an/aus) und „Startgeschwindigkeit ohne
  Gelände“ (km/h).

### Gleitzahl und Landepunkt

- Nur **in der Luft**. Die Sperre „mindestens 30 m über Gelände“ entfällt, die Sperre
  „mindestens 5 km/h über Grund“ bleibt, weil eine Gleitzahl unter Schrittgeschwindigkeit
  auch in der Luft nichts aussagt.
- Der Wind bleibt unabhängig vom Zustand. Er entsteht nur beim Kreisen, und am Boden
  kreist niemand.

## Grenzen (bewusst in Kauf genommen)

- **Seilbahn:** Fährt Sky „bereit“ in der Gondel mit, ist es hoch über dem Gelände, und
  ein Start wird erkannt. An der Bergstation folgt die Landung. Es entsteht ein kurzer
  Scheinflug, der eigentliche Flug ist danach ein eigener und vollständig. Abhilfe: Sky
  erst am Startplatz bereit machen. So nutzt der Nutzer Sky heute schon.
- **Ohne Gelände im Auto:** Hier gilt wie beim Flymaster nur die Geschwindigkeit, im Auto
  beginnt also ein Flug. Das betrifft nur Orte ohne installierte Region.
- **Genauigkeit:** Höhe über Gelände aus GPS-Höhe und Geländemodell (am Brauneck etwa
  13 m pro Pixel) schwankt um einige Meter, an steilen Hängen mehr. Die Schwellen von
  etwa 20 m lassen dafür Luft.

## Ansätze

**Wo gerechnet wird:** wie bei Gleitzahl und Wind in Rust. Ein neues reines Modul in
`flight-core` (Flugzustand: bereit, in der Luft, gelandet) bekommt Fix, Varioreading und
Geländehöhe mit Zeitstempel und meldet Start und Landung mit zurückdatiertem Zeitpunkt.
Die Engine (`vario-engine`) steuert damit Aufzeichnung, Ton und die Gleitzahlsperre und
gibt den Zustand im `Status` aus. Die Oberfläche zeigt nur an. Das Modul ist ohne App
testbar und später in ZeroGrid Base für die Flugauswertung nutzbar (Start und Landung in
alten IGC-Dateien finden).

**Starterkennung:** drei Wege betrachtet:

1. **Nur Geschwindigkeit, wie der Flymaster.** Einfach und vertraut, aber Fehlstart im
   Auto und später Start im starken Wind.
2. **Höhe über Gelände, ohne Gelände Geschwindigkeit** (vorgeschlagen). Kein Fehlstart
   im Auto, Start im Wind wird erkannt. Braucht eine Region, sonst wie 1.
3. **Bewegungsmuster** (Beschleunigung beim Startlauf, Steigen, Kurs). Am genauesten,
   aber schwer zu testen ohne echte Flugdaten und anfällig für Überraschungen. Später
   vielleicht, wenn IGC-Dateien und Sensorlogs echter Starts vorliegen.

**Landeerkennung:** die Flymaster-Regel, zusätzlich „nahe am Boden“ (gewählt), statt
eigener Regeln. Für den Piloten bleibt das Verhalten so gleich zu lesen, der Schutz gegen
das Soaren im Wind ist eine Ergänzung.

## Rahmen

- Architektur wie in `apps/sky/CLAUDE.md`. Keine neuen Abhängigkeiten, kein neuer Thread:
  Die Erkennung braucht je Fix eine Geländeabfrage am eigenen Ort (dieselbe Kachel wie
  für die Luftraumwarnung, also aus dem Zwischenspeicher).
- **Simulator:** `SyntheticFlight` beginnt heute in der Luft. Für Tests und den
  Desktop-Selbsttest braucht er eine Bodenphase am Startplatz, einen Startlauf und eine
  Landung mit Stehen danach. Der Desktop-Simulator soll so einen ganzen Flug am Brauneck
  zeigen.
- **Tests:** synthetische Flüge für Start mit und ohne Wind, Start ohne Gelände, Autofahrt
  (am Boden schnell), Soaren mit null Fahrt über Grund (keine Landung), Toplanden und
  neuer Start, „Stopp“ ohne Start. Echte Starts und Landungen liegen noch nicht vor. IGC-
  Dateien vom Flymaster in `testdata/sky/` wären später der beste Prüfstein.
- **Dokumentation:** neuer Abschnitt in `docs/ALGORITHMEN.md`, Status in
  `apps/sky/CLAUDE.md`.
- Klar benennen, was nur im Simulator geprüft ist. Ob die Erkennung bei echten Starts
  taugt, zeigt erst der Flug.

## Entscheidungen des Nutzers (26.11.2026)

Alle vier Vorschläge übernommen:

1. Starterkennung über **Höhe über Gelände** (Weg 2), ohne Gelände über die
   Geschwindigkeit.
2. **„Ton erst ab Start“** voreingestellt an (wie der Flymaster).
3. Nach der Landung **bereit bleiben** für den nächsten Start.
4. Flüge **ohne erkannten Start** bei „Stopp“ speichern und kennzeichnen.

## Dem Builder überlassen

- Genaue Schwellen und Haltezeiten (um 20 m über Gelände, einige Sekunden; 15 km/h; 30 s
  und 60 s), was „ruhig am Boden“ beim Zurückdatieren heißt.
- Welche Höhe für die Höhe über Gelände (GPS-Höhe oder Druckhöhe, beim Bereitmachen an
  GPS angeglichen).
- Länge des Puffers vor dem Start (um 60 s) und was „nennenswerte Bewegung“ bei „Stopp
  ohne Start“ heißt.
- Wie der Zustand im `Status` und der Flugbeginn in der IGC-Datei abgelegt werden.
- Wie „ohne erkannten Start“ in der Flugdatei oder der Flugliste vermerkt wird.
- Genaue Texte und Gestaltung in der Oberfläche innerhalb des bestehenden Stils.
