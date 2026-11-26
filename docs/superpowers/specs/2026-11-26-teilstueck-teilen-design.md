# Entwurf: Teilstück wählen und in der Mitte teilen (Trail)

Stand: 2026-11-26 · Status: vom Nutzer freigegeben · Größe: kleine Änderung

## Anlass und Ziel

Bei langen Routen aus vielen Teilstücken mit verschiedenen Sportarten will der Nutzer mitten in
der Route eine zusätzliche Etappe einfügen, ohne davor oder danach etwas neu zu setzen. Heute
geht das nur am Ende; die Sportart eines früheren Teilstücks lässt sich nicht ändern (Nicht-Ziel
im Entwurf vom 2026-11-15, dort als „Teilstück anklicken und Profil wählen“ vorgemerkt).
Das Wählen eines Teilstücks braucht später auch das Vorhaben Fotos/Notizen nicht direkt (die
hängen an der Aufzeichnung), ist aber dieselbe Bedienidee.

## Ziele

- Tipp auf die Routenlinie wählt das Teilstück; es wird hervorgehoben, unten erscheint eine
  kleine Karte: Sportart, Länge, Auf-/Abstieg, Knopf „Punkt in der Mitte einfügen“.
- Tipp neben die Linie fügt wie bisher einen Wegpunkt am Ende an (im Setzmodus).
- Ist ein Teilstück gewählt, ändert die Sportart-/Einstellungswahl nur dieses Teilstück;
  nur dieses wird neu gerechnet. Ohne Auswahl gilt wie heute: letztes Teilstück und alle neuen.
- „Punkt in der Mitte einfügen“ setzt einen Wegpunkt auf die Mitte der **berechneten Strecke**
  (halbe Länge entlang der Linie), nicht der Luftlinie. Beide Hälften erben Profil und
  Einstellungen; nur diese zwei werden neu gerechnet. Danach ist der Punkt ein normaler
  Wegpunkt (verschieben, entfernen, Etappenende).

## Nicht-Ziele

- Base (Desktop) bekommt die Bedienung jetzt nicht; die Kernfunktion steht ihm später offen.
- Fotos, Notizen, Export, Sportart je Aufzeichnungsabschnitt: eigenes Vorhaben (siehe unten).

## Anti-Ziele

- Einfügen oder Profilwechsel rechnet nie Teilstücke neu, die sich nicht geändert haben.
- Gespeicherte Routen öffnen weiter unverändert.

## Betrachtete Wege

| Weg | Abwägung |
|---|---|
| **Kern bekommt `insert_waypoint(leg, at)`** (gewählt) | Logik im Rust-Kern wie vom Etappenmodell gefordert; `set_leg(index, leg)` gibt es schon. |
| Oberfläche baut den neuen Request selbst | Widerspricht dem Anti-Ziel „Logik im Kern“. |
| Mitte der Luftlinie statt der Strecke | Punkt läge oft neben dem Weg und die Route würde sich verbiegen. |

Es gab nur einen sinnvollen Weg; der Nutzer hat den Entwurf so freigegeben.

## Entwurf

- **Kern (`core-app/src/plan.rs`):** `PlanRequest::insert_waypoint(leg_index, at) -> Self`:
  fügt `at` als Wegpunkt `leg_index + 1` ein, dupliziert `legs[leg_index]`, verschiebt
  Etappenenden mit höheren Indizes um eins. Dazu eine Hilfe, die aus dem berechneten Teilstück
  (`Leg.line`) den Punkt bei halber Länge liefert. Beides über `core-ffi` für Swift.
- Neu gerechnet werden nur die zwei Hälften (der bestehende Teilstück-Cache greift für den Rest).
- **Trail:** Treffertest auf die Routenlinie (Linienlayer in `RouteLayers`), gewähltes
  Teilstück hervorgehoben, Karte mit Werten und Knopf; `PlanController` hält `selectedLeg`,
  die Sportwahl wirkt bei Auswahl über `set_leg` auf genau dieses Teilstück. Auswahl verschwindet
  bei Tipp ins Leere, beim Leeren oder Laden der Route und wenn das Teilstück nicht mehr
  existiert.
- Selbsttests mit echten Eingaben (Tipp auf die Linie), wie in den bisherigen UI-Tests.

## Dem Bauenden überlassen

Trefferradius auf der Linie, Aussehen der Hervorhebung und der Karte, ob „Rückgängig“ das
Einfügen zurücknimmt (Empfehlung: ja, wenn es mit dem bestehenden Rückgängig einfach geht,
sonst wie heute nur der letzte Punkt).

## Für das nächste Vorhaben (Aufnahme, Fotos, Notizen) festgehalten

Entscheidungen des Nutzers vom 2026-11-26:
- Fotos und Notizen hängen an der **Aufzeichnung** (Ort und Zeit), nicht an der geplanten Route.
- Start der Aufnahme: kurze Auswahl der Sportart, letzte vorausgewählt, „Keine Angabe“ möglich,
  ein Tipp startet. (Ersetzt die Wahl beim Beenden aus dem Entwurf vom 2026-11-24.)
- Sportart wechseln während der Aufnahme nur von Hand; jeder Wechsel beginnt einen neuen
  Abschnitt mit der neuen Sportart.
- Rasterkennung (länger als x im Umkreis y) und längere Unterbrechungen teilen in Abschnitte
  gleicher Sportart. Werte x, y offen.
- Ohne laufende Aufnahme startet der Kameraknopf nur eine Kamera, die in die Fotomediathek
  speichert (iOS kann die System-Kamera-App nicht direkt öffnen).
- Notizen getippt oder gesprochen, nachträglich änderbar, je Abschnitt oder für die ganze Tour;
  Fotos löschbar; Export als Paket (GPX, Fotos, Notizen) für spätere Tourberichte auf Base.
