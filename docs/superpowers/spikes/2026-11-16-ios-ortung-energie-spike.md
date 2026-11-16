# Messversuch: Ortung und Akku beim Aufzeichnen auf dem iPhone

Übergabe für die Sitzung auf dem MacBook. Stand 2026-11-16.

## Worum es geht

Nach I1–I4 (Karte, Höhenlinien, Planung, Freigabe im Heimnetz) kommt als Nächstes das
**Aufzeichnen von Tracks auf dem iPhone**. Später folgt ein Tourenbuch mit Statistik über
Zeiträume.

Der Rust-Kern kann schon aufzeichnen: Recorder mit Start, Pause und Fortsetzen, Segmente,
Wiederaufnahme nach Absturz, Statistik und SQLite (`core-track`, `core-app`). Auf iOS fehlt nur
die Ortung. Swift liefert die Positionen, der Kern macht den Rest.

**Für den Nutzer ist der Akkuverbrauch ein Hauptziel der ganzen App**, bei bestmöglicher
Genauigkeit. Sein Einsatzfall:
- 7+ Tage Radreise ohne Steckdose, Strom nur aus Powerbank und Solarzelle. Je nach Wetter steht
  nur halb so viel Energie zur Verfügung, oder dreimal so viel.
- Ebenso 1–2 Tage ganz ohne Strom.
- „Im Notfall die Genauigkeit runterdrehen statt ganz verzichten.“

Dieser Wegwerf-Test misst, was die Ortung auf dem iPhone wirklich kostet. **Erst danach werden
die Stufen festgelegt und die Aufzeichnung entworfen.**

## Schon entschieden (Gespräch mit dem Nutzer, 2026-11-16)

- **Stufen, umschaltbar auch während einer laufenden Aufzeichnung**, ohne Neustart und ohne
  Lücke. Vorschlag (Werte legt dieser Test fest):
  - **Genau:** GPS voll, alle 1 s (Rad) bzw. 3 s (Wandern), wie heute im Kern
    (`Activity::location_interval_ms`)
  - **Ausgewogen:** GPS voll, ein Punkt alle ~10 m, bei Stillstand keiner
  - **Sparsam:** gröbere Genauigkeit, ein Punkt alle ~50 m
  - **Notfall:** GPS nur zeitweise an (z. B. 30 s alle 5 min), grober Linienzug
- **Pause bei Stillstand, die Strom spart:** Steht man länger als ein paar Minuten, schaltet die
  Ortung auf sparsam und wird bei Bewegung wieder genau. Die Statistik trennt Bewegungszeit
  ohnehin. Der Nutzer vergisst das Pausieren oft.
- **Ein Track pro Abschnitt:** einer pro Tag, oder mehrere, wenn die Aufzeichnung unterbrochen
  wird. Die Bündelung zu einer „Reise“ kommt später im Tourenbuch.
- **Nach der Tour:** Der Track bleibt in der App, auf der Karte, mit Statistik, als GPX
  exportierbar. Abgleich mit Desktop und Server sowie mehrere Benutzerkonten kommen später.
- **Weg A:** klassische `CLLocationManager`-Schnittstelle (ab iOS 16.4), wegen der vollen
  Kontrolle über den Energieverbrauch. **Nicht** `CLLocationUpdate.liveUpdates` (iOS 17).
- Der Desktop bleibt unverändert. Seine Aufzeichnung ist ein Relikt, stört aber nicht.
- Ein Umbau der Oberfläche und ein OLED-Sparmodus kommen später.

## Fragen

1. **Akku pro Stunde** je Stufe bei gesperrtem Bildschirm, im Flugmodus mit Ortung an.
2. **Wie genau** ist der Track je Stufe: Punktabstand, `horizontalAccuracy`, Lücken, Ecken in
   Kurven?
3. **Notfall-Stufe:** Lässt iOS die App im Hintergrund am Leben, während der GPS-Empfänger
   zwischen den Messungen aus ist, und spart das wirklich? Zu prüfen sind diese Varianten:
   - **N1:** Die Ortung läuft weiter, aber zwischen den Messungen mit
     `desiredAccuracy = kCLLocationAccuracyThreeKilometers`, also ohne GPS. Für ~30 s geht sie
     auf `kCLLocationAccuracyBest`. Dieser bekannte Kniff hält die App wach.
   - **N2:** `startMonitoringSignificantLocationChanges` (Funkzellen-basiert). Wahrscheinlich
     nutzlos im Outback ohne Netz, nur zur Einordnung.
   - **Nicht** `stopUpdatingLocation` mit Timer: Im Hintergrund feuert der Timer dann nicht
     mehr. Das nur kurz bestätigen.
4. **Pause bei Stillstand:**
   - `pausesLocationUpdatesAutomatically` mit `activityType` hat einen bekannten Haken: iOS
     setzt die Ortung danach nicht von selbst fort, erst wenn die App in den Vordergrund
     kommt. Das bestätigen oder widerlegen.
   - Alternativ eigene Erkennung: Ist die Position x min stabil, Genauigkeit runter wie bei N1,
     und bei Bewegung (Positionssprung > Genauigkeit) zurück auf die Stufe.
   - Messen: Wie schnell wird die Bewegung erkannt, und was spart das?
5. **Überlebt die Aufzeichnung**, wenn iOS die App beendet, etwa bei Speicherdruck oder nach
   einem Neustart? Für die Rückkehr nach Beendigung gibt es
   `startMonitoringSignificantLocationChanges` oder die Region-Überwachung. Prüfen, ob iOS die
   App damit wieder startet. Der Kern übernimmt danach die Wiederaufnahme.

## Aufbau

Wegwerfcode: Mini-App unter `spikes/ios-location/` (nicht in `apps/`), ohne Rust-Kern, ohne
Tests.

- `UIBackgroundModes: location` und `NSLocationWhenInUseUsageDescription`. Falls „Immer“
  nötig ist, `NSLocationAlwaysAndWhenInUseUsageDescription`, und festhalten, ob das nötig war.
  `allowsBackgroundLocationUpdates = true`, `showsBackgroundLocationIndicator = true`.
- Auswahl der Stufe (Genau, Ausgewogen, Sparsam, N1, N2, Stillstand eigen,
  Stillstand-iOS-Pause) und der Aktivität (Rad 1 s, Wandern 3 s). Umschalten während der
  Messung muss gehen.
- Ein Protokoll je Lauf als CSV im Dokumentenverzeichnis, über die Dateien-App abholbar:
  - je Position: Zeit, Lat, Lon, Höhe, `horizontalAccuracy`, Geschwindigkeit
  - alle 5 min: `UIDevice.current.batteryLevel`, `ProcessInfo.thermalState`, Stufe,
    Hintergrund ja/nein
  - Ereignisse: Pausen, Fortsetzen, Beendigung, Neustart der App
- Die Anzeige: Laufzeit, Punkte, Akku-Start/jetzt, %/h. Bildschirm während der Messung
  gesperrt.

## Messablauf (der Nutzer, draußen)

- iPhone 13 Pro Max, Akku ≥ 80 %, Flugmodus an, Ortung an, keine anderen Apps aktiv,
  Helligkeit egal (Bildschirm aus).
- Jede Stufe **mindestens 1,5 h am Stück** in echter Bewegung (Rad oder zu Fuß), sonst sind die
  1-%-Schritte von `batteryLevel` zu grob. Dieselbe Strecke oder eine ähnliche.
- Dazu eine **Leerlaufmessung**: App ohne Ortung, 1,5 h gesperrt, als Grundlinie.
- Pause bei Stillstand: 20 min Pause mitten in einem Lauf.
- Notfall-Stufe: mindestens 3 h.

## Erfolg

1. Für jede Stufe: %/h (abzüglich Grundlinie), Punktabstand und Genauigkeit in einer Tabelle.
2. Antworten auf Frage 3–5 mit Beleg (Protokollauszug).
3. Daraus ein Vorschlag für die endgültigen Stufen mit geschätzter Laufzeit für einen vollen
   Akku. Beispielrechnung: Wie viele Tage reicht ein Akku plus eine 10-Ah-Powerbank je Stufe
   bei 8 h Fahrt am Tag?

## Ergebnis

*(von der Mac-Sitzung auszufüllen)*
