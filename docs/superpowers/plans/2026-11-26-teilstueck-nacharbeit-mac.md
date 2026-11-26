# Nacharbeit Teilstück teilen (nächste Mac-Sitzung)

Stand: 2026-11-26 · aus der Abschlussprüfung von PR #11 (`feature/leg-split`), gemergt als
5c91140. Der Nutzer hat entschieden: erst mergen, diese Punkte in der nächsten Mac-Sitzung
nachziehen (Swift ließ sich unter Linux nicht bauen). Kleiner Branch, z. B. `fix/leg-selection`.

## Zu beheben

1. **Wichtig – Auswahl überlebt eine fehlgeschlagene Berechnung.**
   `PlanController.computed` (`apps/trail/ZeroGridTrail/Plan/PlanController.swift`, Zweig
   `.failure`) setzt `plan = nil`, `selectedLeg` bleibt. `PlanSheet` zeigt `LegSummary` nur mit
   `plan.plan != nil`, die Auswahl ist also unsichtbar, lenkt aber die nächste Sportwahl über
   `planSetLeg` auf das alte Teilstück statt auf das letzte, und der nächste Kartentipp hebt nur
   die Auswahl auf. Tritt nur auf, wenn `planRoute` als Ganzes wirft (Engine-/Dateifehler).
   Fix: im `.failure`-Zweig `selectLeg(nil)`.
   Test: Unit- oder UI-Test, der `planRoute` scheitern lässt (falls kein einfacher Weg: im
   Selbsttest einen Fehler auslösen) und prüft `selected_leg=-` und dass die Sportwahl das
   letzte Teilstück ändert.
2. **Klein – später Tipp auf die Linie wählt im Setzmodus.** Der Tipp wird 0,3 s verzögert
   zugestellt (Doppeltipp-Warten); wer in der Zeit den Setzmodus einschaltet, hat ein Teilstück
   gewählt, und der nächste Tipp setzt keinen Punkt (gegen die Entscheidung des Nutzers: im
   Setzmodus setzt jeder Tipp einen Punkt). Fix: in `selectLeg` für einen Index
   `guard !picking`.

## Bewusst liegen gelassen (Kleinigkeiten aus der Prüfung)

- Formular zeigt nach `connectStraight`/`reroute` am gewählten Teilstück dessen alte Sportart;
  Zurückwählen ist durch `guard profile != self.profile` gesperrt. Fix wäre: in
  `request.didSet` bei bestehender Auswahl `showInForm(request.legs[selectedLeg])`.
- Während des Rechnens kann ein Tipp auf die alte Linie kurz ein anderes Teilstück wählen.
- Schließen-Knopf in `LegSummary` ist während des Rechnens gesperrt.
- Die Karte zeigt die Sportart nur als Farbpunkt.
- Luftlinienmitte mittelt Breite/Länge (falsch über den 180. Längengrad; praktisch egal).
- „Rückgängig“ nimmt das Teilen nicht zurück (offen seit dem Entwurf).

## Erledigt (2026-11-26, Branch `fix/leg-selection`)

- Punkt 1: `computed` hebt im `.failure`-Zweig die Auswahl auf. Test
  `PlanUITests.testFailedPlanningLetsGoOfTheLeg` mit dem neuen Selbsttest-Schalter
  `-failPlansAfter <n>` (nur DEBUG): nach n berechneten Routen scheitert jede Berechnung als
  Ganzes. Gegenprobe: ohne den Fix schlägt der Test fehl (`selected_leg` bleibt `0`).
- Punkt 2: `selectLeg` wählt im Setzmodus nichts (`guard !picking`). Ohne eigenen Test: das
  Fenster von 0,3 s lässt sich mit XCUITest nicht verlässlich treffen.
- Alle 20 `PlanUITests` im Simulator grün.
