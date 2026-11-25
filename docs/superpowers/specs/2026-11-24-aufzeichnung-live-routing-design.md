# Trail: Aufzeichnung und Live-Routing

Stand 2026-11-24. Löst den Messversuch `docs/superpowers/spikes/2026-11-16-ios-ortung-energie-spike.md` als Voraussetzung ab: Die Messwerte stehen noch aus, deshalb kommt zuerst eine übliche GPS-Einstellung, weitere Profile folgen später. Die Builder-UI in Base (Map-Builder Schritt 3) ist dafür zurückgestellt.

## Absicht

Der Nutzer will unterwegs sofort einen Nutzen: Trail zeichnet die Tour auf und führt ihn auf einer geplanten Route. Kommt er von der Route ab, zeigt Trail einen aktuellen Weg zurück, so wie Google Maps und andere Navi-Apps.

So nutzt er es:
- **Rad:** Das iPhone hängt am Lenker, oft mit eingeschaltetem Display, wie früher bei komoot. Seit der Akku Kapazität verloren hat, ist der Bildschirm öfter aus und nur bei Bedarf an.
- **Wandern und Trekking:** Der Bildschirm ist meist aus.
- Mehrtägige Radreisen ohne Steckdose, Strom nur aus Powerbank und Solarzelle. **Der Akkuverbrauch ist ein Hauptziel der ganzen App** (siehe Messversuch).

## Ziele

- Tracks auf dem iPhone aufzeichnen: Start, Pause, Fortsetzen, Ende, auch bei gesperrtem Bildschirm.
- Nach der Tour: Track in der App auf der Karte, mit Statistik, als GPX exportierbar, umbenennen, löschen.
- Live-Routing auf einer geladenen Route: Abweichung erkennen, Weg zurück zeigen, bei ausgeschaltetem Bildschirm Bescheid geben.
- Live-Routing an- und ausschaltbar. Bei laufender Aufzeichnung standardmäßig an, aber auch dann abschaltbar.

## Nicht-Ziele (später)

- Weitere GPS-Profile (Ausgewogen, Sparsam, Notfall) und Pause bei Stillstand. Sie kommen, wenn die Messwerte da sind. Der Entwurf muss sie zulassen, ohne Umbau.
- Wiedereinstieg auf der Linie selbst oder Vergleich mehrerer Wegpunkte (Ansätze B und C unten).
- Abbiegehinweise und Sprachansagen.
- Tourenbuch, Bündelung zu Reisen, Abgleich mit Desktop und Server.
- Routen über Regionsgrenzen hinweg.
- Änderungen am Desktop. Seine Aufzeichnung bleibt, wie sie ist.

## Was auf keinen Fall passieren darf

- Das Live-Routing verändert die geplante Route: Wegpunkte, ihre Reihenfolge, Teilstücke oder den Speicherstand. Der Weg zurück ist nur eine vorübergehende Anzeige.
- Daten verlassen das Gerät, oder etwas braucht Internet.
- Ein Punkt wird verloren, weil der Bildschirm gesperrt ist oder die App im Hintergrund läuft.
- Eine gerade Linie sieht aus wie ein berechneter Weg zurück.
- Die App meldet sich ständig. Es gibt **eine** Meldung je Abweichung.
- GPS läuft, ohne dass der Nutzer es eingeschaltet hat (Standortknopf, Aufzeichnung oder Live-Routing). Seit dem 2026-11-24 läuft das GPS der Karte nur, solange der Standortknopf an ist.

## Ansatz für das Live-Routing

Betrachtet wurden:

- **A: Zurück zum nächsten noch nicht erreichten Wegpunkt (gewählt).** Bei einer Abweichung plant Trail vom Standort zu diesem Wegpunkt neu, danach gilt der geplante Rest unverändert. Das ist am einfachsten. Liegen die Wegpunkte weit auseinander, kann der Weg zurück einen Umweg bedeuten. Der Nutzer: „In den paar Fällen, wo das in der Realität ein Thema ist, habe ich in einer Minute umgeplant.“
- B: Die nächsten 2–3 Wegpunkte vergleichen, Route dorthin plus geplanter Rest, die kürzeste Summe gewinnt. Das war die erste Idee des Nutzers. Zurückgestellt.
- C: Wiedereinstieg an Punkten auf der geplanten Linie vor dem Nutzer, unabhängig von den Wegpunkten. Zurückgestellt. A soll so gebaut sein, dass B oder C später nur die Wahl des Ziels für den Weg zurück austauschen.

Bei einer Route nur mit Start und Ziel ist der nächste Wegpunkt das Ziel. Der Weg zurück ist dann eine komplett neue Route zum Ziel, wie gewünscht.

Die Logik sitzt im Rust-Kern, Trail liefert nur die Positionen und zeigt an (Entscheidung von Claude). So lässt sie sich unter Linux mit abgespielten GPX-Tracks testen (`GpxReplaySource` gibt es schon). Swift lässt sich unter Linux nicht bauen.

## Vom Nutzer entschieden

- **GPS:** zuerst eine übliche Einstellung für solche Apps, weitere Profile später.
- **Live-Routing:** an- und ausschaltbar. Bei laufender Aufzeichnung standardmäßig an, aber abschaltbar.
- **Hinweis bei Abweichung:** kurze Vibration und eine Mitteilung auf dem Sperrbildschirm, eine je Abweichung. Ein zusätzlicher Signalton ist abschaltbar.
- **Weg zurück:** Ansatz A.
- **Reihenfolge:** Aufzeichnung und Live-Routing vor der Base-Oberfläche.
- **Ansatzpunkt und Restangaben:** wie unter Meilenstein 2 beschrieben. Die Sonderfälle dort hat Claude vorgeschlagen und der Nutzer bestätigt.

## Was es heute gibt

- **Kern:** `core-app` zeichnet schon auf: `start_recording(activity)`, Pause, Fortsetzen, `stop_recording`, Wiederaufnahme nach einem Absturz (`interrupted_recording`, `continue_interrupted`, `finish_interrupted`), `list_tracks`, `track_detail`, `rename_track`, `delete_track`, `export_gpx`. Positionen von außen nimmt `App::handle_fix(Fix)` an. Der Abstand der Punkte richtet sich nach der Aktivität (`Activity::location_interval_ms`: Rad und MTB 1 s, Wandern und Skitour 3 s).
- **FFI:** `core-ffi` reicht davon **nichts** an Swift weiter. `on_position` und `on_recording` des Listeners sind leer.
- **Trail:** Die Ortung läuft heute über MapLibre (`showsUserLocation`) und einen `CLLocationManager` in `MapView.swift`, dazu eine einmalige Abfrage in `SearchBridge.swift`. Es gibt nur die Erlaubnis „Beim Verwenden“, keinen Hintergrundmodus. Geplante Routen sind `RoutePlan` mit Wegpunkten, Teilstücken (`Leg` mit Linie und Profil) und Etappen. Planen läuft über `plan_route` mit Valhalla auf dem Gerät, etwa 40 ms je Teilstück auf dem iPhone 13 Pro Max.

## Meilenstein 1: Ortung und Aufzeichnung

### Ortung

- Ein eigener Ortungsdienst in Trail (`CLLocationManager`, klassische Schnittstelle, wie im Messversuch beschlossen) liefert die Punkte an den Kern (`handle_fix`). Er wird von Karte, Aufzeichnung und Live-Routing gemeinsam genutzt und läuft, solange mindestens eines davon ihn braucht.
- **Übliche Einstellung** (Entscheidung von Claude): `desiredAccuracy = kCLLocationAccuracyBest`, `distanceFilter = kCLDistanceFilterNone`, `activityType` passend zur Aktivität (Rad: `.fitness` bzw. `.otherNavigation`, der Bauende wählt), `pausesLocationUpdatesAutomatically = false`. Den Abstand der gespeicherten Punkte regelt der Kern wie heute über die Aktivität.
- Die Einstellung steckt hinter einem benannten Profil („Standard“), damit die späteren Profile nur weitere Einträge sind und während der Aufzeichnung umschaltbar werden können.
- **Hintergrund:** `UIBackgroundModes: location`, `allowsBackgroundLocationUpdates = true` nur während Aufzeichnung oder Live-Routing, `showsBackgroundLocationIndicator = true`. Die Erlaubnis „Beim Verwenden“ reicht, solange die Ortung im Vordergrund gestartet wird. Ob iOS doch „Immer“ verlangt, hält der Test auf dem iPhone fest.
- Läuft nur die Karte (Standortknopf), bleibt alles wie heute: kein Hintergrund.

### Aufzeichnung

- Ein Aufnahmeknopf auf der Karte. Beim Start wählt der Nutzer die Aktivität (Rad, MTB, Wandern, Skitour). Vorauswahl ist die zuletzt genutzte (Entscheidung von Claude).
- Während der Aufnahme: die Linie live auf der Karte, eine kleine Leiste mit Zeit, Strecke und Höhenmetern, Pause/Fortsetzen und Beenden. Beenden fragt nach.
- Ist beim Start eine Route geladen, startet das Live-Routing mit (Meilenstein 2).
- Wurde die App während einer Aufnahme beendet, fragt sie beim nächsten Start: fortsetzen oder als fertigen Track speichern. Der Kern kann das schon.
- **Tracks:** eine Liste wie die gespeicherten Routen (Name, Datum, Strecke), dazu eine Ansicht mit dem Track auf der Karte und der Statistik. Umbenennen, Löschen und GPX-Export über das Teilen-Menü von iOS.

So sieht die Leiste aus (Skizze, Gestaltung frei):

```
┌──────────────────────────────────────┐
│ ● 1:42 h   23,4 km   ↑ 410 m          │
│ [ Pause ]              [ Beenden ]    │
└──────────────────────────────────────┘
```

## Meilenstein 2: Live-Routing

### Ablauf

1. **An- und ausschalten:** Ein Schalter, sobald eine Route geladen ist. Bei laufender Aufzeichnung mit geladener Route ist er von selbst an. Ohne geladene Route gibt es kein Live-Routing.
2. **Ansatzpunkt beim Einschalten** (vom Nutzer bestätigt): Trail sucht den Punkt der ganzen Route, der dem Standort am nächsten liegt. Ab dort gilt der nächste Wegpunkt. Am zweiten Tag einer Reise am Hotel geht es also mit Etappe 2 weiter. Zu Hause ist der nächste Punkt der Start, und Trail führt dorthin. Für den seltenen Fall, dass die Route zweimal nah an derselben Stelle vorbeiführt, kann der Nutzer einen Wegpunkt antippen und „Ab hier führen“ wählen. Dasselbe gilt, wenn die Route unterwegs geändert wird.
3. **Fortschritt:** Bei jedem Punkt bestimmt der Kern den nächsten Punkt auf der aktiven Linie und daraus die gefahrene Strecke. Er sucht nur in einem Fenster um den letzten Fortschritt, damit bei Hin- und Rückweg auf derselben Straße oder bei Schleifen nichts springt. Ein Wegpunkt gilt als erreicht, sobald der Fortschritt hinter ihm liegt.
4. **Abweichung:** Mehrere Punkte hintereinander zu weit neben der aktiven Linie. Ungenaue Punkte zählen nicht. Die Schwellen legt der Bauende fest und hält sie in `docs/ALGORITHMEN.md` fest, als Anhaltspunkt etwa 40 m und 3 Punkte bzw. 10 s.
5. **Weg zurück:** Der Kern plant vom Standort zum nächsten noch nicht erreichten Wegpunkt. Er nimmt das Profil des Teilstücks, auf dem der Nutzer zuletzt war, und gibt die Fahrtrichtung mit, damit Valhalla nicht unnötig wenden lässt. Ab dem Wegpunkt gilt der geplante Rest. Die aktive Linie ist dann der Weg zurück plus der Rest.
6. **Wieder auf der Route:** Ist der Nutzer wieder nahe an der geplanten Linie, auch vor dem Wegpunkt, fällt der Weg zurück weg.
7. **Wieder abgekommen:** Weicht er vom Weg zurück ab, gilt das als neue Abweichung.
8. **Kein Weg zurück:** Scheitert die Berechnung, etwa am Rand der Region, zeigt Trail das an und versucht es beim nächsten Punkt nicht sofort wieder, sondern erst nach einer Wartezeit oder deutlicher Bewegung. Es gibt keine gerade Linie als Ersatz.
9. **Ziel erreicht:** Das Live-Routing endet von selbst, die Aufzeichnung läuft weiter.

### Sonderfälle der geladenen Route (Entscheidungen von Claude)

- **Gerade verbundene Teilstücke** (Profil `Manual`, „gerade verbinden“): Der Fortschritt läuft auf der geraden Linie weiter, aber es gibt dort keine Abweichungsmeldung. Wer gerade verbindet, geht bewusst querfeldein. Braucht ein Weg zurück ein Profil, nimmt er das des letzten berechneten Teilstücks davor, sonst das danach.
- **Gescheiterte Teilstücke** (ohne Linie): Sie gelten als Lücke, ohne Abweichungsmeldung. Der Fortschritt springt zum Wegpunkt am Ende der Lücke, sobald der Nutzer der Linie danach nahekommt.
- **Route wird unterwegs geändert** (Wegpunkt verschoben, Profil gewechselt, andere Route geladen): Das Live-Routing übernimmt die neue Route und bestimmt den Fortschritt neu wie beim Einschalten. Die Bearbeitung wird nicht gesperrt, Umplanen unterwegs ist ausdrücklich gewollt. Wird die Route geschlossen, endet das Live-Routing.
- **Route wird während einer Aufzeichnung geladen:** Das Live-Routing geht von selbst an. Hat der Nutzer es während dieser Aufzeichnung ausgeschaltet, bleibt es für diese Aufzeichnung aus, auch nach einem Neustart der App.

Neu gerechnet wird nur bei einer Abweichung, nicht in festen Abständen. Das spart Akku und Rechenzeit.

### Anzeige

- Die geplante Route bleibt sichtbar. Der schon gefahrene Teil ist blasser, der Weg zurück hat eine eigene, durchgezogene Farbe.
- Eine Leiste mit Reststrecke und Restzeit nach dem Routing, bei einer Route mit Etappen in zwei Zeilen: bis zum Ende der aktuellen Etappe und bis zum Ziel (vom Nutzer bestätigt). Ohne Etappen nur bis zum Ziel. Die Höhenmeter für den Rest sind optional, der Bauende entscheidet.
- Mit eingeschaltetem Live-Routing folgt die Karte dem Standort. Wer die Karte verschiebt, beendet das Folgen. Ein Knopf holt es zurück (Entscheidung von Claude).

### Hinweis bei Abweichung

- **Eine** lokale Mitteilung mit kurzer Vibration je Abweichung, etwa „Abseits der Route. Weg zurück zu Wegpunkt 4: 650 m.“ Bei eingeschaltetem Bildschirm genügt die Anzeige in der App mit Vibration, ohne Mitteilung (Entscheidung von Claude).
- Der Signalton ist ein Schalter in den Einstellungen, Standard an (Entscheidung von Claude, damit man ihn in der Tasche hört).
- Die Erlaubnis für Mitteilungen fragt Trail beim ersten Einschalten des Live-Routings ab. Ohne Erlaubnis bleibt nur die Anzeige in der App.

## Akku

Mit Aufzeichnung oder Live-Routing läuft das GPS voll. Das ist die übliche Einstellung, bis die Messwerte da sind. Das Live-Routing selbst kostet wenig: Fortschritt und Abweichung sind einfache Geometrie je Punkt, Routenberechnungen gibt es nur bei einer Abweichung. Im Hintergrund zeichnet Trail die Karte nicht neu.

## Dem Bauenden überlassen

- Aufteilung im Kern: eigenes Modul in `core-app` oder eigene Crate, und wie die Wahl des Ziels für den Weg zurück austauschbar bleibt.
- Die genauen Schwellen (Abstand, Zahl der Punkte, Fenster für den Fortschritt, Wartezeit nach einem Fehlschlag). Begründet mit abgespielten echten Tracks.
- Wie Positionen und Zustand des Live-Routings über FFI zu Swift kommen (Listener oder Abfrage).
- Gestaltung von Aufnahmeleiste, Trackliste und Navigationsleiste im Stil der vorhandenen Bedienleisten, im OLED-Dunkelmodus mit.

## Prüfung

- **Kern:** Tests unter Linux mit abgespielten GPX-Tracks: auf der Route, Abweichung, Rückkehr vor dem Wegpunkt, Schleife und Hin- und Rückweg, gescheiterte Berechnung, Route nur mit Start und Ziel, GPS-Sprünge.
- **Trail:** Unit- und UI-Tests im Simulator mit simulierter Bewegung (GPX im Simulator).
- **Draußen** (der Nutzer): je eine Runde mit dem Rad bei eingeschaltetem und bei gesperrtem Bildschirm, dabei absichtlich abbiegen. Zu prüfen: Wird kein Punkt verloren, kommt die Mitteilung genau einmal, stimmt der Weg zurück? Akkuverbrauch pro Stunde grob notieren und in `docs/MESSPROTOKOLL.md` eintragen.

## Nach dem ersten Test auf dem iPhone (2026-11-24, vom Nutzer entschieden)

Der Nutzer fand nicht heraus, wie eine Aufnahme startet: Die Wahl der Aktivität startete sie, ohne dass das erkennbar war. Er will das Aufzeichnen einfacher und unabhängig von allem anderen. Das ersetzt die entsprechenden Punkte oben.

- **Ein Knopf, eine Aufnahme.** Der Aufnahmeknopf auf der Karte startet sofort, ohne Frage. Die Leiste zeigt Zeit, Strecke und Höhenmeter, dazu „Pause“/„Weiter“ und „Beenden“.
- **Beim Beenden:** „Track speichern?“ mit optionaler Sportart (Rad, MTB, Wandern, Skitour oder „Keine Angabe“, vorausgewählt), „Speichern“ und „Verwerfen“ (Verwerfen fragt nach). Bis zur Antwort ist der Track schon gespeichert, damit nichts verloren geht.
- **Name:** systematisch, `2026-11-24 10:05 Rad` (Start, Sportart; ohne Angabe nur Datum und Uhrzeit). Umbenennen bleibt möglich.
- **Punkte jede Sekunde** für alle Aufnahmen, weil die Sportart erst am Ende feststeht (Entscheidung von Claude). Die Statistik rechnet nach dem Speichern mit den Werten der gewählten Sportart neu.
- **Unerwarteter Abbruch:** Beim nächsten Start speichert Trail das Teilstück von selbst als Track und sagt es („Eine Aufzeichnung wurde unterbrochen und als Track gespeichert.“). Kein „Fortsetzen“ mehr.
- **GPS länger weg:** Die Aufnahme läuft weiter, nach einer Lücke von mehr als 2 Minuten beginnt ein neues Teilstück. Kommt die Position wieder, meldet Trail die Lücke („Kein GPS für 5 min. Die Aufzeichnung läuft weiter.“).
- **Aufnahme und Live-Routing sind getrennt.** Eine Aufnahme startet keine Führung und eine Führung keine Aufnahme. Die Regel „bei laufender Aufzeichnung standardmäßig an“ und die Einstellung `guide_off_track` entfallen.
- **Tipp auf die Karte:** Ist die eigene Position bekannt und der Modus „+ Punkt“ aus, bietet ein Tipp auf die Karte (abseits von Suchtreffern) „Route hierher“ und „Hierher führen“ an, wie bei einem Suchtreffer.
- Die Bedienung insgesamt wird später überarbeitet.

## Nach dem Test draußen (2026-11-25, vom Nutzer entschieden)

- **Umplanen beim Abkommen:** Statt eines Wegs zurück neben der Route ersetzt Trail die Route ohne Rückfrage durch eine neue ab der Position; die alte verschwindet von der Karte, auch der gefahrene Teil. Betroffen ist nur das aktive Teilstück.
- **Nur Gespeichertes bleibt:** Routen bleiben nur durch „Speichern“, Tracks nur durch die Aufnahme. Alles andere liegt im Arbeitsspeicher und wird ohne Kommentar verworfen, auch beim Ende der App. Die Warnung „Nicht gespeichert“ entfällt.
- **Gespeicherte Route nach dem Umplanen:** „Speichern“ legt eine neue Route an; die gespeicherte bleibt unverändert (Vorschlag von Claude, vom Nutzer nicht widersprochen).
