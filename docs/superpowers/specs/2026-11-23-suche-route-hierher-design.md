# Map-Builder Schritt 2: Offline-Suche und „Route hierher“

Stand 2026-11-23. Baut auf Schritt 1 auf (`2026-11-23-kartenvorlagen-suchindex-design.md`).

## Absicht

ZeroGrid soll ein eigenes System sein, das keine Daten nach außen sendet und dauerhaft ohne Internet funktioniert. Der wichtigste Fall: **Der Nutzer kommt spät in einer fremden Stadt an, tippt ohne Netz sein Hostel ein, findet es und bekommt eine Route dorthin.**

Der zweite Fall kommt aus Radreisen: unterwegs umplanen. Mit Komoot war das für den Nutzer ein Krampf, weil eingefügte Punkte nicht einfach mit dem Punkt davor und danach verbunden wurden und ohne Netz nichts mehr ging. Die Bedienung muss deshalb ohne Einarbeitung verständlich sein.

## Ziele

- Suche nach Orten, Straßen und POIs in Base und Trail, vollständig offline.
- Schnellwahl nach Kategorie.
- „Route hierher“ von jedem Treffer, mit wenigen Tipps.
- Verständliche Meldungen und immer ein Ausweg statt einer Sackgasse.

## Nicht-Ziele (später)

- Hausnummern und Sprachansagen.
- Automatisches Umplanen bei Abweichung von der Route.
- Einen Punkt in ein bestimmtes Teilstück einfügen und verschieben.
- Routen über Regionsgrenzen hinweg.
- Suche in Sky. Sky hat kein Routing.
- Builder-UI und Vorlagenwahl in Base (Schritt 3).

## Was auf keinen Fall passieren darf

- Die Reihenfolge der Wegpunkte wird verändert. **Sie ist strikt.** Jedes Teilstück läuft von Punkt `i` nach `i + 1`, wie heute in `plan_route`.
- Suche oder Routing brauchen Internet oder senden Daten.
- Eine gerade Linie sieht wie eine berechnete Route aus. Der vorhandene gestrichelte Platzhalter für gescheiterte Teilstücke bleibt, weil er sichtbar „nicht berechnet“ bedeutet.
- „Route hierher“ ersetzt eine ungespeicherte Route ohne Hinweis. Die Warnung gilt nur für diesen Dialog. Löschen, Laden und App-Ende bleiben wie heute, das hat der Nutzer als eigene Verantwortung akzeptiert.

## Ansatz

Betrachtet wurden:

1. **Suche im gemeinsamen Rust-Kern, Apps zeigen nur an (gewählt).** Entspricht dem Routing, beide Apps liefern dieselben Treffer.
2. Suche in jeder App einzeln. Verworfen: doppelte Logik in Swift und TypeScript, die auseinanderläuft.
3. Zuerst nur eine App. Verworfen: Der Kern wird ohnehin gebraucht, der Hauptfall braucht Trail, und Base ist am schnellsten testbar.

Reihenfolge des Baus: Kern, dann Base (unter Linux testbar), dann Trail (der Nutzer baut und testet auf dem Mac).

## Vom Nutzer entschieden

**Suchleiste.** In Trail immer sichtbar oben auf der Karte, die vorhandene Werkzeugleiste rückt darunter. Beim Suchen öffnet sich ein Bereich von oben, die Karte bleibt unten teilweise sichtbar. In Base steht die Leiste fest oben in der Seitenleiste.

**„Route hierher“ bei offener Route.** Ist keine Route offen, wird sofort vom eigenen Standort aus berechnet. Ist eine offen, fragt die App:

```
Route hierher
Nicht gespeichert: Bei „Neue Route“ geht die alte verloren.
[ Neue Route ]
[ Als nächstes Ziel anhängen ]
[ Abbrechen ]
```

Der Warnsatz erscheint nur, wenn die Route wirklich ungespeichert ist. „Anhängen“ setzt das Ziel ans Ende der Route.

**Kein nutzbarer Standort.** Das Ziel wird gesetzt, und die App sagt: „Tippe auf der Karte, wo die Route beginnen soll.“ Der getippte Punkt wird Startpunkt.

**Keine Routing-Daten für ein Stück.** Die App sagt: „Für dieses Stück gibt es keine Routing-Daten.“ Darunter steht der Knopf **„Als gerade Linie verbinden“**. Erst dieser Knopf stellt das Teilstück auf das Profil `manual` um.

## Entscheidungen des Entwurfs (vom Nutzer korrigierbar)

**Sortierung nach Nähe.** Bezugspunkt ist immer die **Kartenmitte**. Folgt die Karte dem Standort, ist das die Umgebung des Nutzers. Plant er zu Hause Cusco, kommen Treffer aus Cusco. Die Apps übergeben also immer einen Punkt. Damit entfällt im UI auch das bekannte Problem falscher Treffer ohne Standort (`Maximilianstraße München`).

**Reihenfolge der Treffer:**

1. Orte, deren Name alle Suchwörter als ganze Wörter enthält, größere zuerst, bei gleicher Größe die näheren.
2. Alles andere nach Entfernung.

So zeigt „Cusco“ zuerst die Stadt und „Hostel“ die nächsten Unterkünfte.

**Suchwörter für Kategorien (vom Nutzer entschieden).** Die Suche versteht Wörter für die Art eines Ortes auf Deutsch, Englisch und Spanisch, zum Beispiel Hostel, Hotel, Unterkunft, Apotheke, Farmacia, Tankstelle, Grifo, Supermarkt. Besteht die ganze Eingabe aus so einem Wort, kommen zu den Namenstreffern die nächsten POIs der passenden Kategorie hinzu, ohne Doppelte und gemeinsam nach Entfernung sortiert. Die Zuordnung liegt im Kern, gilt beim Lesen und braucht keinen Neubau. Unterschieden wird nur nach Kategorie, „Hostel“ zeigt also auch Hotels. Eingaben aus mehreren Wörtern bleiben reine Namenssuche.

**Größe der Orte.** Das Index-Werkzeug (`tools/search-index`) schreibt den Rang des Orts in `subtype`. `SEARCH_FORMAT` bleibt 1, damit vorhandene Pakete lesbar bleiben. Orte ohne Rang stehen innerhalb der ersten Gruppe hinten. Für die bessere Sortierung müssen Karten einmal neu gebaut werden.

**Schreibweisen.** Suchbegriffe werden wie die Namen in Varianten aufgelöst: `München` findet auch gespeichertes `Muenchen`, `Str.` und `Straße` sind gleichwertig, auch als eigenes Wort (`Haupt Str.`). Das ändert nur das Lesen und braucht keinen Neubau.

**Suche über Regionen.** Gesucht wird in allen installierten, aktivierten Regionen mit Suchindex. Ein doppelter Treffer gilt als gleich, wenn Art, Kategorie und normalisierter Name übereinstimmen und die Punkte höchstens 30 m auseinanderliegen.

**Regionen ohne Suchindex.** Das Ergebnis nennt sie. Die Apps zeigen unter den Treffern: „Alpenraum-Ost ist nicht durchsuchbar. Baue die Region neu.“ Ein beschädigter Index verhindert die Treffer der anderen Regionen nicht und wird genauso genannt.

**Eingabe.** Gesucht wird ab 2 Zeichen während des Tippens, kurz verzögert und abseits des UI-Threads. Eine langsame alte Antwort darf keine neuere ersetzen. Maximal 30 Treffer im UI, der Kern begrenzt das Limit auf 1 bis 100.

**Kategorien.** Die Schnellwahl erscheint, solange das Suchfeld leer ist: Unterkunft, Essen, Lebensmittel, Apotheke, Tankstelle, Camping, Parken, Sehenswertes. `Retail` bleibt im Index, bekommt aber keinen Knopf. POIs ohne Namen zeigen ihre Kategorie als Titel.

**Treffer.** Ein Tipp schließt die Liste, zentriert die Karte, setzt eine Markierung und zeigt eine Karte mit Name, Art, Ort, Entfernung zur Kartenmitte und „Route hierher“.

**Nutzbarer Standort.** Eine Position ist nutzbar, wenn sie höchstens 2 Minuten alt ist und eine aktivierte Region mit Routing-Daten sowohl die Position als auch das Ziel enthält. Andernfalls gilt der Fall „Kein nutzbarer Standort“. In Base zählen auch GPSD und die GPX-Wiedergabe als Position.

**Profil.** „Route hierher“ nimmt das zuletzt benutzte Profil außer `manual`, auch nach einem Neustart der App. Vor der ersten Nutzung gilt der bisherige Standard der App. Danach lässt es sich wie gewohnt ändern.

**Ungespeichert.** Eine Route ist ungespeichert, wenn sie Wegpunkte hat und sich von ihrem zuletzt gespeicherten oder geladenen Stand unterscheidet. Das gibt es heute noch nicht und kommt in den Kern, damit beide Apps gleich entscheiden.

**Gerade Linie.** Der Knopf erscheint in zwei Fällen: bei fehlenden Routing-Daten und wenn Routing-Daten da sind, aber keine Route gefunden wird (etwa ein Hostel an einem nicht angebundenen Weg). Im zweiten Fall lautet der Text: „Keine Route gefunden. Versuche ein anderes Profil, verschiebe den Punkt oder verbinde gerade.“ Bei anderen Fehlern wie einer unpassenden Valhalla-Version erscheint er nicht. Der Kern meldet den Grund deshalb maschinenlesbar. Nach dem Knopf ist das Teilstück ein normales `manual`-Teilstück mit dessen vorhandener Darstellung und Zeitschätzung. Ein neues Kennzeichen im Routenformat gibt es nicht.

**GPS sucht noch (vom Nutzer entschieden).** Gibt es noch keine frische Position, zeigt die App „Standort wird gesucht …“ mit den Knöpfen „Start selbst wählen“ und „Abbrechen“. Kommt eine Position, wird die Route automatisch berechnet. Nach 30 Sekunden ohne Position bittet die App, den Start anzutippen. Hat der Nutzer „Start selbst wählen“ gedrückt, ändert eine später eintreffende Position nichts mehr. Fehlt die Standort-Erlaubnis, fragt Trail beim ersten Mal das System. Bei Ablehnung, oder in Base ohne aktive Positionsquelle, geht es ohne Wartezeit direkt zum Antippen. Trail braucht dafür eine eigene Abfrage der Position, weil es heute nur die Position der Kartenansicht liest.

**Standort außerhalb der Karte.** Ist die Position bekannt, liegt aber nicht mit dem Ziel in derselben Region, sagt die App: „Dein Standort liegt nicht auf derselben Karte wie das Ziel. Tippe, wo die Route beginnen soll.“ Hat das Ziel gar keine Region mit Routing-Daten, kommt nach dem Tippen die Meldung mit „Als gerade Linie verbinden“.

## Was schon da ist

- `crates/core-search`: `SearchIndex::open`, `search`, `by_category`.
- `crates/core-maps`: `SEARCH_FILE`, `manifest.search`, `routing_tiles`.
- `crates/core-app/src/plan.rs`: `plan_route` berechnet je Teilstück und nutzt berechnete Teilstücke wieder.
- Trail: `apps/trail/ZeroGridTrail/Plan/PlanController.swift`, `Map/MapScreen.swift`, `Map/MapToolbar.swift`.
- Base: `apps/base/ui/src/route.ts`, `api.ts`, `main.ts`, `apps/base/src-tauri/src/commands.rs`.

## Dem Bauenden überlassen

- Schnittstellen und Typen in `core-app`, `core-ffi` und den Tauri-Kommandos.
- Wie Suchindizes geöffnet und zwischengespeichert werden. Sie werden mit `immutable=1` gelesen, daher dürfen beim Aktualisieren, Deaktivieren oder Löschen einer Region **keine alten Verbindungen** weiterleben.
- Gestaltung im Rahmen der Entscheidungen oben, passend zum Stil der jeweiligen App.

## Prüfung

- Rust-Tests: Sortierung, Schreibweisen, Duplikate, Regionen ohne oder mit defektem Index, Limit, Standortregel, Fehlergrund, Ungespeichert-Regel.
- Base-Selbsttests mit echten Mausereignissen. Sie dürfen keine echten Daten anfassen, dafür **`OUTDOOR_DATA_DIR` und `OUTDOOR_DATA`** setzen.
- Antwortzeit messen und in `docs/MESSPROTOKOLL.md` festhalten, mit dem größten vorhandenen Testpaket. Ziel: unter 300 ms auf dem Rechner. Auf dem iPhone misst der Nutzer im Handtest.
- Trail: Nach Kernänderungen `apps/trail/scripts/build-core.sh` auf dem Mac, dann Handtest des Hostel-Falls im Flugmodus.

## Fertig, wenn

In Trail im Flugmodus: Hostel in Cusco suchen, antippen, „Route hierher“, Route erscheint. Dasselbe funktioniert in Base. Die Fälle offene Route, kein Standort und fehlende Routing-Daten führen jeweils zu einem verständlichen nächsten Schritt.
