# Übergabe: Aufzeichnung und Live-Routing in Trail auf dem Mac

Stand: 2026-11-24, Branch `feature/recording-live-routing`. **Noch kein Merge und kein PR.** Beides kommt erst nach Meilenstein 2 und nur nach Rückfrage.

Entwurf: `docs/superpowers/specs/2026-11-24-aufzeichnung-live-routing-design.md`
Plan: `docs/superpowers/plans/2026-11-24-aufzeichnung-live-routing.md`

## Was fertig ist (Teil A, Kern, unter Linux)

Tasks 1–5 des Plans, jede einzeln umgesetzt und geprüft. `cargo test --workspace` und clippy sind grün.

- **Task 1:** `RouteRequest.heading_deg` gibt Valhalla die Fahrtrichtung für den ersten Punkt mit.
- **Task 2:** `core-app/src/guide/line.rs`, die Route als Linie mit Fortschritt, Wegpunkt- und Etappenpositionen.
- **Task 3:** `guide/guide.rs`, die Führung als Zustandsautomat. Werte und Regeln stehen in `docs/ALGORITHMEN.md`, Abschnitt 7.
- **Task 4:** `guide/app.rs`, die Führung in `App`: Positionen, Planwechsel und Kopplung an die Aufzeichnung (Schlüssel `guide_off_track`).
- **Task 5:** `core-ffi/src/track.rs` und `guide.rs`. Aufzeichnung, Tracks und Führung für Swift. `CoreListener` hat zwei neue Methoden: `on_recording` und `on_guidance`.

**Trail baut auf diesem Stand nicht**, bis `CoreHost.swift` die zwei neuen Listener-Methoden umsetzt. Das ist der erste Schritt von Task 6.

## Reihenfolge auf dem Mac

1. Stand holen und bauen:
   ```bash
   git fetch origin
   git switch feature/recording-live-routing
   apps/trail/scripts/build-core.sh
   cd apps/trail && xcodegen generate && cd ../..
   ```
2. Tasks 6–8 des Plans (Ortungsdienst, Aufzeichnen, Tracks). Danach anhalten: Meilenstein 1, Test auf dem iPhone.
3. Tasks 9–11 (Führen, Hinweis bei Abweichung, Test draußen). Danach anhalten: Meilenstein 2.
4. Gesamtprüfung des ganzen Branches durch einen frischen Prüfer, dann PR und Merge `--no-ff`, alles nach Rückfrage.

Umsetzung wie in Teil A: je Task ein Umsetzer, danach ein Prüfer (vom Nutzer gewählt).

## Entscheidungen aus Teil A, die für Trail gelten

- Die Führung läuft weiter, wenn die Aufzeichnung endet. Sie ist unabhängig schaltbar. Trail sollte sichtbar machen, dass sie noch läuft.
- Solange der Nutzer nach dem Einschalten noch nicht auf der Route war, führt der Weg zurück zum Ansatzpunkt: zu Hause zum Start, bei „Ab hier führen“ zum gewählten Wegpunkt. Danach gilt der nächste Wegpunkt voraus.
- Wird die Route während der Führung geändert, setzt der Kern die Führung neu an, wie beim Einschalten.
- **Ein Listener darf aus `on_guidance` heraus nicht synchron `handle_fix` aufrufen.** Der Kern hält dabei eine Sperre. Trail reicht die Callbacks ohnehin auf den Main-Thread weiter.
- `GuidanceInfo.deviation` zählt jede neue Abweichung hoch. Trail meldet sich, wenn der Wert wächst, und nur dann.

## Kleinigkeiten für die Gesamtprüfung (zurückgestellt)

- Task 3: Eine Abweichung kurz vor dem Ziel ohne Wegpunkt voraus zählt mit, obwohl es keinen Weg zurück gibt. Der Testrouter verwirft die Fahrtrichtung, kein Test prüft, dass sie weitergegeben wird. `project_near` sammelt Kandidaten in einen `Vec`. `guide.rs` hat rund 1100 Zeilen (zur Hälfte Tests) und `#[allow(clippy::module_inception)]`. Die Abweichungszeit startet erst mit der Meldung, das Fenster wächst deshalb etwas vorsichtig.
- Task 4: Der Listener wird unter der Sperre `guide_turn` aufgerufen. Eine alte gespeicherte Route wird beim Laden während der Führung zweimal angesetzt. Ein Speicherfehler in `switched_off` wird verschluckt. `plan_change_drops_stale_way_back` würde ohne Zeitlimit hängen. Es fehlen Tests für „Ziel erreicht“ mit laufender Aufzeichnung und für `guide_from_waypoint`.
- Task 5: kleine Doppelungen in `track.rs`, ein locker prüfender Listener-Test.
