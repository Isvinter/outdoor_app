# Entwurf: Routen aus Teilstücken, Etappen und Aktivitätsfarben

Stand: 2026-11-15 · Status: zur Freigabe

## Anlass und Ziel

Der Nutzer plant lange Touren: Bikepacking München–Rom/Athen/Barcelona (über 1 000 km),
gemischte Touren wie „Bike and Ski“ und „Bike and Climb“, Skidurchquerungen, oft ohne
Handyempfang. Heute schickt die App bei jedem Klick **alle** Wegpunkte als eine Anfrage an
Valhalla:

- Valhalla lehnt Anfragen ab, deren Summe der Luftlinien zwischen den Wegpunkten das Limit des
  Profils übersteigt: Wandern 250 km, Fahrrad 500 km, Auto 5 000 km (Voreinstellung von
  Valhalla, nie angepasst). Bei 270 km Route war damit Schluss.
- Ein Fehler verwirft die ganze Route. Beim Test des Nutzers kam sie nach „Rückgängig“ nicht
  zurück. Das ließ sich am Router nicht nachstellen; die Ursache ist offen und entfällt mit
  diesem Entwurf als Fehlerklasse.
- Eine Route hat genau ein Profil; gemischte Touren gehen nicht.

Ziel: Eine Route besteht aus Teilstücken mit je eigenem Profil, gegliedert in Etappen, die
der Nutzer setzt; jede Änderung rechnet nur die betroffenen Teilstücke neu.

## Ziele

- **Teilstück** = Weg zwischen zwei aufeinanderfolgenden Wegpunkten, mit eigenem Profil und
  eigenen Einstellungen. Ein neuer Wegpunkt rechnet nur das neue Teilstück.
- „Rückgängig“ verwirft nur das letzte Teilstück; Verschieben eines Wegpunkts rechnet nur die
  beiden angrenzenden Teilstücke, Entfernen eines mittleren Punkts nur das neue verbindende.
- Profil und Einstellungen im Formular gelten für das **letzte** Teilstück und alle danach
  hinzugefügten (Wunsch des Nutzers).
- **Etappen:** Der Nutzer markiert Wegpunkte als Etappenende. Statistik (Distanz, Auf- und
  Abstieg, Zeit) je Etappe und gesamt; die Gesamtwerte sind die Summe der Etappen.
- **Höhenprofil** farbig nach Aktivität der Teilstücke, mit knapper Legende darunter.
- Ein fehlgeschlagenes Teilstück verwirft nichts: Die übrigen bleiben sichtbar, das fehlerhafte
  ist markiert, die Meldung nennt es.
- Lange Routen (1 000 km, einige hundert Wegpunkte) bleiben flüssig.

## Nicht-Ziele

- Zug- oder Transferetappen (Nutzer: „muss die App aktuell nicht berücksichtigen“).
- Routen über mehrere Regionen. Eine Route liegt auf einer Karte; über Kartengrenzen teilt der
  Nutzer in eigene Routen (Nutzer: das ist in Ordnung).
- Profil eines früheren Teilstücks nachträglich ändern. Weg dorthin: Punkte zurücknehmen und
  neu setzen. Kann später über „Teilstück anklicken und Profil wählen“ kommen.
- Fotos, Notizen, Tourberichte (spätere Vorhaben; das Modell soll sie nicht verbauen).

## Anti-Ziele

- Ein Fehler in einem Teilstück löscht nie bereits berechnete Teilstücke.
- Bestehende gespeicherte Routen gehen nicht verloren und öffnen weiter.
- Die Logik liegt im Rust-Kern, nicht in der Oberfläche (die mobilen Clients sollen sie
  ohne Nachbau nutzen).

## Rahmenbedingungen

- Valhalla-Limits gelten je Anfrage; eine Anfrage hat jetzt nur noch zwei Punkte. Ein einzelnes
  Teilstück über 250 km Luftlinie zu Fuß bleibt abgelehnt; das ist bei Klickabständen kein
  praktisches Problem.
- Gemessen auf `alpenraum-ost` (4,7 GB): Route über 243 km mit 8 Punkten 0,14 s. Ein Teilstück
  dauert also Millisekunden; die Kosten langer Routen liegen in Höhenprofil, Statistik und
  Übertragung an die Oberfläche.

## Betrachtete Wege

| Weg | Abwägung |
|---|---|
| 1. Oberfläche setzt die Route aus Einzelanfragen zusammen | Schnell gebaut; Logik, Speichern und Export lägen in TypeScript und müssten für iOS/Android nachgebaut werden. |
| **2. Der Kern kennt Teilstücke und Etappen** (gewählt) | Mehr Umbau; mobile Clients und spätere Tourberichte bekommen alles aus dem Kern. |

Entscheidung des Nutzers: Weg 2.

## Entwurf

### Modell (Kern)

Anfrage der Oberfläche an den Kern, ersetzt `RouteRequest` für die Planung:

```
RoutePlanRequest {
  waypoints:  [LatLon]                         // n ≥ 2
  legs:       [{ profile, options }]           // n − 1, Teilstück i von Punkt i nach i+1
  stage_ends: [usize]                          // Indizes innerer Wegpunkte, aufsteigend
}
```

Antwort `RoutePlan` (neu gefasst):

- `legs`: je Teilstück Profil, Einstellungen, Linie, Distanz, Zeit, Auf-/Abstieg und ein
  Fehler (`None` oder Code und Text). Ein fehlerhaftes Teilstück hat keine Linie.
- `elevation`: Höhenprofil über die ganze Route, jeder Punkt mit dem Index seines Teilstücks
  (daraus färbt die Oberfläche).
- `stages`: je Etappe Wegpunktbereich, Distanz, Auf-/Abstieg, Zeit, beteiligte Aktivitäten.
- `total`: Summe der Etappen; ist ein Teilstück fehlerhaft, zählt es nicht und `total` sagt das.
- `region`, `max_mtb_scale` wie bisher.

Zeit je Teilstück: zu Fuß (Wandern, Manuell) DIN 33466 aus Distanz und Auf-/Abstieg des
Teilstücks; sonst die Zeit des Routers. Etappen- und Gesamtzeit sind Summen. (Die
DIN-Formel ist nicht additiv; je Teilstück gerechnet und summiert ist eine bewusste,
einfache Näherung.)

### Neuberechnung

Der Kern hält einen Speicher berechneter Teilstücke, Schlüssel: Region, Start, Ziel, Profil,
Einstellungen. Bei jeder Anfrage rechnet er nur Teilstücke, die nicht im Speicher sind,
Höhenprofil und Statistik je Teilstück ebenso. Der Speicher ist begrenzt (Größenordnung
einige tausend Teilstücke) und wird beim Löschen oder Ersetzen der Region geleert, wie die
Router heute. Die Oberfläche schickt weiterhin den ganzen Zustand – sie muss nicht wissen, was
neu zu rechnen ist.

Alle Wegpunkte müssen in **einer** Region mit Routingdaten liegen (wie heute). Teilstücke mit
Profil „Manuell“ (gerade Linie, Skitour) brauchen keinen Router.

### Speichern und Export

- `plan_json` in `routes` speichert künftig Anfrage und Ergebnis im neuen Format mit
  Formatnummer. Alte Einträge (ein Profil) werden beim Laden umgedeutet: alle Teilstücke mit
  diesem Profil, keine Etappenenden. Keine Schemaänderung nötig.
- Die Spalte `profile` erhält bei gemischten Routen die beteiligten Profile, durch `+`
  verbunden (`mtb+hike`), sonst wie bisher ein Profil.
- GPX: je Etappe ein `<trk>` (Name „Etappe n“ bzw. Routenname bei einer Etappe), darin je
  Folge gleicher Aktivität ein `<trkseg>`; der Track trägt die Aktivität, wenn sie in der
  Etappe einheitlich ist. Wegpunkte mit Etappenende werden zusätzlich `<wpt>`.

### Oberfläche

- Profilauswahl und Regler wirken auf das letzte Teilstück und alle neuen; das Formular zeigt
  das Profil des letzten Teilstücks.
- Entfernen eines mittleren Wegpunkts verbindet seine Nachbarn mit dem Profil des Teilstücks
  **vor** ihm; war er Etappenende, entfällt die Marke. Verschieben behält die Profile.
  Etappenindizes verschiebt die Oberfläche mit.
- Klick auf einen Wegpunkt öffnet ein kleines Menü: „Etappenende“ (an/aus) und „Entfernen“.
  (Heute entfernt ein Klick sofort; der zusätzliche Schritt verhindert Versehen und schafft
  Platz für die Etappe.) Etappenenden haben ein eigenes Markersymbol.
- Linie auf der Karte farbig nach Aktivität, gleiche Farben wie im Höhenprofil; ein
  fehlerhaftes Teilstück erscheint als gestrichelte Luftlinie in Warnfarbe.
- Statistik: Gesamtwerte wie heute oben; darunter eine Zeile je Etappe (nur bei mehr als einer
  Etappe) mit Distanz, Auf-/Abstieg, Zeit und Aktivitätsfarben.
- Höhenprofil farbig nach Aktivität; darunter eine Legende nur der vorkommenden Aktivitäten
  (Farbfeld und Name, eine Zeile). Etappengrenzen als dünne senkrechte Linien.
- Farben: je Aktivität eine feste Farbe, gut unterscheidbar auf der Karte und im Profil, auch
  im Dunkelmodus.

### Prüfung

- Kern-Tests: neues Teilstück rechnet nur eines neu (Router-Attrappe zählt Aufrufe);
  „Rückgängig“ und Verschieben rechnen nur die betroffenen; fehlerhaftes Teilstück lässt die
  anderen stehen; Etappenstatistik summiert sich zur Gesamtstatistik; alte gespeicherte Route
  lädt; GPX mit zwei Etappen und gemischter Aktivität.
- Valhalla-Test: Route mit Teilstücken über das bisherige Luftlinienlimit (Fixture-Region
  reicht dafür nicht; Prüfung einmal echt auf `alpenraum-ost`, Messung ins Messprotokoll).
- Selbsttest der Oberfläche mit echten Mausereignissen: Punkte setzen, Profil wechseln,
  Etappenende über das Menü setzen, Rückgängig; Farben und Legende im Bild prüfen.
- Messung: 1 000-km-Route mit ~300 Punkten – Zeit je neuem Punkt und Größe der Antwort.

## Dem Umsetzer überlassen

- Größe und Verdrängung des Teilstück-Speichers.
- Ob die Antwort bei sehr langen Routen das Höhenprofil ausdünnt (wenn die Messung es nahelegt).
- Genaue Farben, Gestaltung von Menü, Etappenmarker und Etappenzeilen im bestehenden Stil.
- Ob `RouteRequest` in `core-routing` bleibt (für ein Teilstück) oder umbenannt wird.
