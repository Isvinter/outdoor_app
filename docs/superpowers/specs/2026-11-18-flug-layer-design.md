# Flug-Layer im Region-Builder – Entwurf (kurz)

Stand 2026-11-18. Größe: kleine Änderung.

## Warum

Die Gleitschirm-App Vario (github.com/Isvinter/Vario) liest die Regionspakete dieses Projekts
über `core-maps` als Git-Abhängigkeit und zeigt sie mit ihrem Stil „Flugkarte“. Ihr fehlen
Stromleitungen und Start-/Landeplätze (Vario Phase 6). Gebraucht wird das vor dem Flug zur
Planung, vor allem aber im Flug bei einer ungeplanten Außenlandung und bei XC-Flügen. Deshalb
zählt jedes Seil über einer Wiese, nicht nur Überlandleitungen.

## Ziele

Ein neues Vektor-Layer `flying` in `outdoor.pmtiles` mit:

| kind | Geometrie | OSM | Attribute | ab Zoom |
|---|---|---|---|---|
| `power_line` | Linie | `power=line` | `voltage` (höchster Wert in kV, Ganzzahl), `name` | 10 |
| `power_minor_line` | Linie | `power=minor_line` | `voltage` | 12 |
| Seilbahn-`kind` = OSM-Wert (`cable_car`, `gondola`, `chair_lift`, `drag_lift`, `goods`, `zip_line`, …) | Linie | `aerialway=*` außer `pylon`, `station` | `name` | 11 (`goods`, `zip_line`: 12) |
| `takeoff`, `landing`, `toplanding`, `training` | Punkt (Flächen als Mittelpunkt) | `sport=free_flying` mit `free_flying:site=*`; ältere Schreibweisen `free_flying=*` und `sport=paragliding`/`hang_gliding` | `name`, `ele`, `paragliding`, `hanggliding` (bool, wenn erfasst), `direction` (Text wie erfasst, z. B. `NW` oder `270-320`), `winch` (bool) | 9 |
| `mast`, `tower`, `chimney`, `wind_turbine` | Punkt | `man_made=mast\|tower\|chimney`; `power=generator` + `generator:source=wind` oder `man_made=wind_turbine` | `height` (m, Ganzzahl), `name` | 12 |

Alle Seilbahnen kommen in `flying`, obwohl die Basiskarte einen Teil davon schon enthält. So
hat Vario alle Seile und Leitungen in einem Layer und muss nicht wissen, welche Arten
OpenMapTiles mitbringt (Materialseilbahnen fehlen dort vermutlich). *Meine Entscheidung.*

Startplätze ohne `free_flying:site` zählen als `takeoff`, wenn sie sonst eindeutig als
Fluggelände getaggt sind. *Meine Entscheidung; das genaue Tag-Mapping legt der Builder anhand
echter Daten der Testregion fest.*

## Nicht-Ziele

- Lufträume (kommen gesondert, Vario Phase 7).
- Schutzgebiete mit Lande-/Überflugverbot (in OSM zu lückenhaft).
- Bewertung von Landefeldern (Wald/Wiese/Acker liegen schon in `landcover`; Stilfrage).
- Anzeige in den Stilen der outdoor_app (Desktop, iOS). Erst wenn das Flugbuch hierher kommt.
- Änderungen am Vario-Stil: eigene Sitzung im Vario-Projekt.
- Bauzeit-Optimierung des Builders (eigenes Vorhaben, später).

## Anti-Ziele

- Die bestehenden Layer `trail`, `route`, `piste` und `poi` oder die Stile der outdoor_app
  ändern sich sichtbar.
- Ein zusätzlicher Durchlauf über die OSM-Daten (mehr Bauzeit).
- Änderungen am Paketformat, Manifest oder an `core-maps`. Vario muss nur seinen Stil
  ergänzen.

## Ansätze

- **A – neue `kind`s im bestehenden Layer `outdoor`/`poi`:** kein neues Layer, aber Flugdaten
  vermischt mit Wanderdaten; jeder Stil müsste sie aktiv herausfiltern.
- **B – eigenes Layer `flying` in derselben `outdoor.pmtiles` (gewählt):** ein Durchlauf, keine
  Format- oder `core-maps`-Änderung, saubere Trennung im Stil.
- **C – eigene Datei `flying.pmtiles` mit eigenem Bauschritt:** am saubersten getrennt, kostet
  aber einen zweiten Planetiler-Lauf und Änderungen an Manifest, `core-maps` und Vario.

Gewählt: B (vom Nutzer bestätigt).

## Umsetzung (Rahmen)

- `scripts/tiles/OutdoorOverlay.java`: Layer `flying` ergänzen, Layer-Doku im Kopf
  aktualisieren; Linien wie bei `trail` zusammenführen.
- `scripts/build-region.sh`: Kommentar zu `outdoor.pmtiles` ergänzen. Kein neuer Schritt.
- Bestehende Regionen werden neu gebaut (vom Nutzer akzeptiert); für die Prüfung reicht
  `--steps outdoor` an `isarwinkel-achensee` (Brauneck mit Start- und Landeplätzen).
- Prüfen: Kacheln dekodieren und zeigen, dass `flying` die erwarteten Objekte am Brauneck
  enthält und die übrigen Layer unverändert sind (Objektanzahl vorher/nachher).
- `PLAN.md`: Layer `flying` beim Abschnitt Basiskarte erwähnen.

## Dem Builder überlassen

Genaues Tag-Mapping für Altschreibweisen, Spannungs-Parsing (`110000;20000` → 110),
Zusammenführen der Linien, Werkzeug zum Dekodieren der Kacheln.
