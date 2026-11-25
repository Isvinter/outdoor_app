# Gleitzahl, Landepunkt und Wind für ZeroGrid Sky

Stand: 25.11.2026. Code, Kommentare und Commits auf Englisch, sichtbare Texte auf Deutsch.

## Absicht

Der Nutzer fliegt seit kurzem allein (A-Schein) und will Richtung Streckenfliegen und
Hike & Fly, wo Außenlandungen vorkommen. Bei seinem fünften Flug vom Brauneck drehte direkt
nach dem Wegfliegen nach Süden der Wind auf etwa 25 km/h Nord (nachlassender Föhn,
einströmende Klarluft aus dem Vorland). Ohne Gerät musste er die Lage nach Gefühl
einschätzen und landete im Lee außen. Eine Gleitzahl und ein Punkt auf der Karte, wo er
bei gleichbleibenden Bedingungen auf das Gelände trifft, hätten ihm geholfen, die Lage
schneller zu erfassen.

Die drei Anzeigen sind eine zusätzliche Hilfe neben dem, was der Pilot selbst wahrnimmt.
Sie sind schwankend und nicht immer aussagekräftig; das ist dem Nutzer bewusst. Sky ist
kein zertifiziertes Instrument.

Dem Nutzer ist ein ruhiges, aufgeräumtes Display wichtig: Was gerade nichts aussagt, wird
ausgeblendet.

## Ziele

1. **Gleitzahl über Grund** (GZ): Strecke über Grund geteilt durch Höhenverlust in einem
   gleitenden Zeitraum. Der Zeitraum ist in den Einstellungen wählbar.
2. **Landepunkt**: Die aktuelle Gleitlinie wird auf dem aktuellen Kurs verlängert, bis sie
   das Gelände (`dem.pmtiles` der installierten Regionen) trifft. Dieser Punkt erscheint
   auf der Karte. Liegt er außerhalb des sichtbaren Kartenausschnitts, zeigt ein kleiner
   Hinweis am Kartenrand Richtung und Entfernung.
3. **Wind aus dem Kreisen**: Nach ganzen Kreisen schätzt Sky Windrichtung und -stärke und
   zeigt dazu eine Verlässlichkeit von 1 bis 3 Sternen.

## Nicht-Ziele

- Kein Lee, keine Winddrehung, keine Scherung, kein Sicherheitsabstand in der
  Landepunktrechnung: Sie extrapoliert bewusst „stumpf“ den aktuellen Gleitflug.
- Kein Gleitbereich (Fläche „von hier aus erreichbar“) beim Kreisen – spätere Idee.
- Keine Erreichbarkeit von Landeplätzen aus dem Flug-Layer („Landeplatz erreichbar mit
  GZ x“) – spätere Idee.
- Kein Wind aus Kreisstücken im gleitenden Fenster (Verfahren 2 unten) – vielleicht später
  als wählbare Option, wenn Flugtests zeigen, dass der Wind zu spät kommt.
- Keine Gleitzahl durch die Luft (gegenüber Luftmasse); kein Sollfahrt-/McCready-Rechner.
- Kein Ton für diese Werte.

## Was auf keinen Fall passieren darf

- Ein Landepunkt oder eine Gleitzahl erscheint, obwohl die Werte gerade nichts aussagen
  (Kreisen, Steigen, kein Gelände bekannt, kein GPS) – lieber nichts zeigen als etwas
  Falsches.
- Ein alter Wind wird mit voller Sternzahl gezeigt.
- Die neuen Anzeigen verdecken das Luftraum-Warnbanner oder verdrängen Steigwert und Höhe.
- Die Rechnung bremst Vario-Ton, Aufzeichnung oder Luftraumwarnung.

## Verhalten

### Gleitzahl

- GZ = horizontale Strecke über Grund ÷ Höhenverlust, beide über den gewählten Zeitraum.
- Zeitraum zur Auswahl: **10, 20, 30, 60 s**, voreingestellt **20 s**. Abwägung: Lange
  Zeiträume sind ruhig, zeigen nach einem Windsprung aber lange noch die alte Zahl; kurze
  reagieren schnell, sind aber unruhiger (iOS-Barometer nur ca. 1 Hz).
- Angezeigt nur im **Geradeausflug** und bei **Sinken**. Beim Kreisen, beim Steigen oder
  Nullschieber und ohne GPS: ausgeblendet bzw. „–“. Nach dem Kreisen erscheint sie wieder,
  sobald ein ganzer Zeitraum geradeaus geflogen wurde.
- **Sperren gegen Fehlanzeigen am Boden** (Nutzerentscheidung): GZ und Landepunkt
  erscheinen nur ab etwa **5 km/h über Grund** und mindestens etwa **30 m über dem
  Gelände** (ohne bekanntes Gelände: nur die Geschwindigkeitssperre). Am Boden hält vor
  allem die Sperre über Gelände Gehen heraus;
  eine Talfahrt mit der Seilbahn hoch über dem Tal kann durchrutschen. Zuerst waren 15 km/h
  vorgeschlagen; beim Planen fiel auf, dass dann genau der Brauneck-Fall verschwindet
  (Trimmfahrt um 36 km/h gegen 25 km/h Wind ergibt etwa 11 km/h über Grund). Gehen
  schließt schon die Sperre über Gelände aus. Der Nutzer senkte die Sperre dann auf 5 km/h:
  Am Brauneck-Sattel blieb in Böen über 30 km/h fast keine Fahrt über Grund, und auch bei
  normalen Flügen gibt es am Landeplatz Phasen mit Schrittgeschwindigkeit über Grund. Der Nutzer startet Sky ohnehin direkt vor dem Start und
  nutzt Sky nur zum Fliegen; eine echte Start-/Landeerkennung kommt später als eigenes
  Feature und ersetzt dann diese Sperren.
- Anzeige mit einer Nachkommastelle (`6,8`), sehr große Werte gedeckelt (Wert für die
  Grenze: siehe Entscheidungen des Builders).

### Landepunkt

- Start: aktuelle Position und Höhe. Richtung: Kurs über Grund im Zeitraum. Je Schritt
  entlang des Kurses sinkt die Gleitlinie um Schrittlänge ÷ GZ; der erste Punkt, an dem sie
  das Gelände erreicht oder unterschreitet, ist der Landepunkt.
- Da Strecke und Kurs über Grund gemessen sind, steckt der aktuelle Wind schon darin.
- Kein Punkt, wenn: keine GZ (siehe oben), Gelände entlang der Linie unbekannt (außerhalb
  der Regionen), oder kein Treffer innerhalb der Höchstreichweite.
- Neu berechnet einmal pro GPS-Fix (ca. 1 Hz), in einem **eigenen kleinen Thread**
  (Nutzerentscheidung): Die Engine übergibt Position, Höhe, Kurs und GZ; der Thread sucht
  den Treffer im Gelände und legt das Ergebnis für den Status ab. Der Punkt kommt dadurch
  höchstens einen Fix später, dafür kann eine Geländesuche über noch nicht geladene
  Höhenkacheln nie Ton, Aufzeichnung oder Luftraumwarnung aufhalten. Liegt schon eine
  neuere Anfrage vor, wird nur die neueste gerechnet (keine Warteschlange).
- Karte: ein **Kreuz** am Landepunkt, ohne Linie (Entwurf B).
- Außerhalb des sichtbaren Ausschnitts: kleiner Hinweis am Kartenrand in Richtung des
  Punkts mit Pfeil und Entfernung (`▲ 9,4 km`), Stil wie die übrigen Kartenboxen.

### Wind

- Verfahren: **ganze Kreise** (siehe Ansätze). Nach jedem vollen Kreis (ca. 360°
  Kursänderung) wird aus den Geschwindigkeitsvektoren über Grund der Mittelpunkt bestimmt;
  der Versatz ist der Wind. Weitere Kreise werden zur Schätzung hinzugemittelt.
- **Welche Kreise zählen** (Entscheidung von Claude, wegen Windsprüngen wie am Brauneck):
  Gemittelt werden nur die Kreise der **aktuellen Thermik**, ältere Kreise darin mit
  weniger Gewicht. In einer neuen Thermik bleibt der alte Wind mit einem Stern weniger
  stehen, bis der erste neue Kreis ihn ersetzt. Weicht ein neuer Kreis stark vom bisherigen
  Mittel ab, gewinnt der neue.
- Erster Wert erst nach dem ersten ganzen Kreis. Im Geradeausflug bleibt der letzte Wert
  stehen und altert.
- **Sterne (1–3)** aus: Anzahl ausgewerteter Kreise, wie gut die Punkte zu einem Kreis
  passen, Alter der Schätzung. Ab einem Höchstalter verschwindet der Wind ganz.
- Intern wie in der Luftfahrt üblich: Richtung, **aus der** der Wind kommt, Stärke in m/s.
  Anzeige: Pfeil zeigt, **wohin** der Wind weht (wie ein Windsack), Stärke in km/h.

### Anzeige (Entwurf B, vom Nutzer gewählt)

```
┌──────────────────────────────┐
│ +0,4 m/s · 1840 m  [Folgen]  │   Box oben links: Steigwert · Höhe,
│ GZ 6,8             ┌──────┐  │   darunter GZ (nur wenn gültig)
│                    │  ↙   │  │   Windbox rechts unter „Folgen“:
│        ✕           │18km/h│  │   Pfeil, km/h, ★★☆ (nur wenn Wind da)
│                    │ ★★☆  │  │
│                    └──────┘  │   ✕ = Landepunkt
│          ▲ (Schirm)          │
├──────────────────────────────┤
│ Instr. │ Karte │ Flüge │ Einst│
└──────────────────────────────┘
```

- Instrumente-Seite: zwei neue Kacheln **„GZ“** und **„Wind“** neben Höhe, km/h, Kurs.
- Einstellungen: Auswahl „Gleitzahl-Zeitraum“ (10/20/30/60 s).
- Dunkelmodus: dieselben Elemente mit den Farben des Dunkelmodus.
- Was nicht gültig ist, wird ausgeblendet (Windbox, GZ-Zeile, Kreuz, Randhinweis).

## Ansätze

**Wo gerechnet wird** – ein sinnvoller Weg: in Rust. Neue reine Module in `flight-core`
(Gleitzahl, Kreiserkennung/Wind, Landepunkt gegen `Terrain`), aufgerufen von der Engine
(`vario-engine`, `Processor::handle` bei jedem GPS-Fix). Die Ergebnisse kommen als neue
Felder in `Status`; die Oberfläche zeichnet nur. Das folgt den Regeln aus
`apps/sky/CLAUDE.md` (Zeit kommt mit den Messwerten, testbar ohne Oberfläche,
wiederverwendbar in ZeroGrid Base für die Flugauswertung). Rechnen in JavaScript wurde
verworfen: nicht testbar und gegen die Trennung von Kern und Oberfläche.

**Wind** – zwei Verfahren betrachtet:

1. **Ganze Kreise auswerten** (gewählt): bewährt, ruhige Werte, gut mit synthetischen
   Flügen testbar. Nachteil: erst nach dem ersten ganzen Kreis ein Wert.
2. **Gleitendes Fenster über Kreisstücke** (~30 s): schneller, auch beim Achtern am Hang,
   aber leichter zu täuschen (Fahrtänderung in der Kurve) und unruhiger – gegen den Wunsch
   nach einem ruhigen Display. Später evtl. als Option.

**Anzeige** – drei Entwürfe gezeigt (A: alles in der Box oben links; B: eigene Windbox mit
Pfeil; C: Leiste unten). Gewählt: **B**, ergänzt um den Randhinweis aus C. C fiel auch
weg, weil die Leiste unten mit dem Luftraum-Warnbanner kollidiert.

## Rahmen

- Plattform und Architektur wie in `apps/sky/CLAUDE.md`; keine neuen großen
  Abhängigkeiten.
- Leistung: GZ und Wind sind ein paar Rechenschritte je Fix im Engine-Thread. Der
  Landepunkt braucht je Fix einige hundert Höhenabfragen; `core-maps` hält nur 24
  dekodierte Höhenkacheln (`CACHE_SIZE` in `crates/core-maps/src/dem.rs`), ein Fehlgriff
  liest und dekodiert von der Platte. Deshalb läuft die Suche im eigenen Thread (siehe
  Landepunkt). Der Zwischenspeicher bleibt unverändert (die Konstante gilt für den Code,
  den auch ZeroGrid Base nutzt).
- Neuer Thread = kleine Abweichung von der bisherigen Architektur; vom Nutzer am 25.11.2026
  freigegeben.
- Die Engine kennt das Gelände schon (`Processor::set_terrain`, `RegionTerrain` in
  `apps/sky/src-tauri/src/maps.rs`); ohne Regionen ist es `NoTerrain` → kein Landepunkt.
- Der Simulator (`sensor-sim`, `SyntheticFlight`) kennt noch keinen Wind. Für Tests und den
  Desktop-Selbsttest braucht er einen einstellbaren konstanten Wind (Geschwindigkeit über
  Grund = Fahrt durch die Luft + Wind).
- In `testdata/sky/` liegen derzeit keine IGC-Flüge, nur Sensorlogs; Tests stützen sich
  zunächst auf synthetische Flüge.
- Dokumentation: neuer Abschnitt in `docs/ALGORITHMEN.md`; Status in `apps/sky/CLAUDE.md`.
- Klar benennen, was nur im Simulator geprüft ist. Ob die Werte im echten Flug taugen,
  zeigt erst der Flugtest des Nutzers.

## Dem Builder überlassen

- Genaue Werte der Sperren (um 5 km/h, um 30 m über Gelände).
- Gewichtung der Kreise und Schwelle für „weicht stark ab“.
- Genaue Erkennung von Geradeausflug und Kreis (Schwellen für Drehrate/Kursänderung).
- Mindesthöhenverlust und Deckel für die GZ; Verhalten bei Nullschieber.
- Welche Höhe für den Landepunkt (GPS-Höhe MSL oder Druckhöhe) – das Gelände ist MSL.
- Schrittweite und Höchstreichweite der Geländesuche.
- Regeln für die Sterne und das Höchstalter des Winds.
- Wie der Gleitzahl-Zeitraum gespeichert wird (in `VarioSettings`/`settings.txt` oder eigene
  Datei).
- Genaue Gestaltung von Kreuz, Windpfeil und Randhinweis innerhalb des bestehenden Stils.
