# Übergabe: Trail-Suche auf Mac und iPhone

Stand: Ende 2026-11-23, Branch `feature/search-route-here`, auf GitHub gepusht. **Noch kein Merge und kein PR.** Beides kommt erst, wenn Mac- und iPhone-Teil abgeschlossen sind, und nur nach Rückfrage.

## Was fertig ist

- **Kern und Base:** Suche, Radius 2 bis 50 km, Trefferpunkte, „Zurück“ und „Route hierher“. Vom Nutzer in Base von Hand getestet und für gut befunden.
- **Trail:** dieselben Funktionen in `apps/trail/ZeroGridTrail/Search/`, angebunden in `MapScreen`, `MapView`, `PlanController` und `ZeroGridTrailApp`.

**Trail wurde noch nie für iOS kompiliert.** Unter Linux liefen nur die 28 Logiktests aus `SearchControllerTests` und `RouteHereFlowTests`, mit Ersatz für Combine, CoreLocation und die Bindings. SwiftUI-Ansichten, MapLibre-Aufrufe und die Verdrahtung sind ungeprüft. Kleine Build-Fehler sind zu erwarten.

## Reihenfolge

1. Stand holen:
   ```bash
   git fetch origin
   git switch feature/search-route-here
   ```
2. Kern und Projekt bauen, Tests laufen lassen:
   ```bash
   apps/trail/scripts/build-core.sh
   cd apps/trail && xcodegen generate && cd ../..
   apps/trail/scripts/test.sh
   ```
3. Build-Fehler beheben, dann die GPS-Änderung umsetzen (unten).
4. Handtest auf dem iPhone im Flugmodus (unten).
5. Ergebnis und Antwortzeit der Suche in `docs/MESSPROTOKOLL.md` eintragen.
6. Gesamtprüfung:
   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   npm --prefix apps/base/ui run build
   ```
   `failure_carries_the_tail_of_the_output` in `region-builder` schlägt auch auf `main` zufällig fehl.
7. Nach Rückfrage: PR erstellen, lokal mit `--no-ff` mergen, pushen.

## GPS-Änderung (vom Nutzer beschlossen)

Heute schaltet `MapController` nach erteilter Erlaubnis `showsUserLocation` ein (`showLocationIfAllowed`, auch in `locationManagerDidChangeAuthorization`). MapLibre lässt das GPS dann dauerhaft laufen, auch nach „Route hierher“.

Ziel: **Das GPS läuft nur, wenn der Nutzer es anfordert.** Vor dem Umbau kurz festlegen, wann der Standortpunkt wieder ausgeht, zum Beispiel nach dem Zentrieren oder über den Standortknopf. `PositionRequest` in `Search/SearchBridge.swift` stoppt bereits sauber.

## Handtest auf dem iPhone

- Hostel in Cusco suchen, Treffer antippen, „Route hierher“.
- Trefferpunkt auf der Karte antippen, „Zurück“ führt zu Liste und Ausschnitt. Kategorien bleiben erhalten.
- Mit offener, ungespeicherter Route: Warnung, „Neue Route“ und „Als nächstes Ziel anhängen“.
- GPS sucht noch: „Start selbst wählen“, danach kommt eine Position, nichts springt.
- Beim Wählen des Starts einen vorhandenen Wegpunkt antippen.
- Route ändern, während das GPS noch sucht: Der Ablauf bricht ab, die Route bleibt.
- Ziel außerhalb der Karte des Standorts.
- Stück ohne Routing-Daten, „Als gerade Linie verbinden“.
- Tastatur mit dem dauerhaften unteren Panel, auch auf die Höhe der Trefferliste achten.
- Querformat: „Abbrechen“ und die anderen Knöpfe bleiben erreichbar.
- Treffer landen nicht hinter Suchpanel oder unterem Panel.

## Bewusst offen

- Eine ungenaue erste GPS-Position wird als Start akzeptiert.
- Fehler bei der Kategoriesuche zeigen nur eine leere Liste.
- Überlagerte Kamerabewegungen könnten als Bewegung des Nutzers gelten und den Suchbereich neu setzen.
- Das Angebot „Als gerade Linie verbinden“ sitzt über der Karte statt im `PlanSheet`. Bei einem fehlgeschlagenen Stück erscheint es zusammen mit der roten Meldung.
- Löschen bis auf einen Buchstaben verwirft den Suchbereich.
- Ein Treffer, der in den 500 ms vor dem Einpassen geöffnet wird, führt per „Zurück“ zum Ausschnitt davor.

## Hinweise für die Arbeit

- Mit dem Nutzer auf Deutsch und in einfacher Sprache sprechen. Nach jedem Meilenstein anhalten und zeigen.
- Push nur nach einem Meilenstein und nach einmaliger Rückfrage.
- Entwürfe und Pläne: `docs/superpowers/specs/2026-11-23-suche-route-hierher-design.md`, `docs/superpowers/specs/2026-11-23-suche-umkreis-trefferkarte-design.md`, `docs/superpowers/plans/2026-11-23-suche-route-hierher.md` (Task 10).
