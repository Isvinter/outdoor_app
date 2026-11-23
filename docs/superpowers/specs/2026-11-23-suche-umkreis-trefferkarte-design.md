# Suche: Umkreis, Treffer auf der Karte und „Zurück“

Stand 2026-11-23. Ergänzt `2026-11-23-suche-route-hierher-design.md` auf dem Branch
`feature/search-route-here`. Wo sich beide widersprechen, gilt dieses Dokument.

## Warum

Der Nutzer hat die Suche in Base getestet. Bei „camping“ will er sofort sehen, wo die Treffer
liegen, und nach dem Öffnen eines Treffers ohne neue Suche zur Liste zurückkommen. Die Suche
soll sich außerdem auf die Gegend beziehen, die er gerade ansieht, statt immer die ganze Karte
zu durchsuchen. Das ist ein Schritt zu einem runden Kartendienst.

## Ziele

- Alle Treffer der Liste sind als Punkte auf der Karte zu sehen.
- Die Karte stellt sich von selbst auf die Treffer ein.
- „Zurück“ führt vom gewählten Treffer zur vorherigen Liste und zum vorherigen Ausschnitt.
- Der Suchradius folgt der Zoomstufe, mindestens 2 km, höchstens 50 km.

## Nicht-Ziele

- Trail bekommt die Oberfläche hier noch nicht. Der gemeinsame Kern ist danach bereit, Trail
  übernimmt die Regeln in seinem eigenen Schritt (Task 8).
- Kein neues Indexformat und kein Neubau der Karten.
- Keine Gruppierung dicht liegender Punkte. Bei höchstens 30 Treffern ist sie nicht nötig.
- Kein sichtbarer Radius-Kreis und keine Einstellung für den Radius.

## Darf nicht passieren

- Eine Stadt ist von zu Hause aus nicht auffindbar, weil sie außerhalb des Radius liegt.
- Die Karte springt beim Tippen so, dass sich der Suchbereich mit jedem Buchstaben verschiebt.
- Ein Klick auf einen Trefferpunkt setzt beim Routenplanen versehentlich einen Wegpunkt.

## Ansätze

**Wie der Umkreis angewendet wird**

1. Strikter Radius für alles. Verworfen: Orte wären von weiter weg nicht mehr zu finden.
2. Radius nur mit Ausnahme für Orte, sonst „Nichts gefunden“. Verworfen: Namen und Adressen
   wären umständlich zu erreichen.
3. **Vom Nutzer gewählt:** Orte immer auf der ganzen Karte. Kategorien, POIs und Straßen im
   Radius. Bei einer Texteingabe ohne Treffer im Umkreis kommen die nächsten Treffer von weiter
   weg mit Hinweis.

**Wie sich die Karte verhält**

1. **Vom Nutzer gewählt:** automatisch auf die Treffer einpassen.
2. Karte bleibt stehen, dazu ein Knopf „Alle zeigen“. Verworfen: Treffer außerhalb des
   Ausschnitts blieben zunächst unsichtbar.

## Verhalten

**Radius.** Halbe Diagonale des sichtbaren Kartenausschnitts, begrenzt auf 2 bis 50 km.

**Bezugspunkt bleibt stehen.** Kartenmitte und Radius werden beim Beginn einer Suche festgehalten
und gelten, bis der Nutzer die Karte selbst bewegt oder die Suche leert. So verschiebt das
automatische Einpassen weder das Tippen noch den Wechsel zwischen Kategorien.

**Texteingabe.**

1. Liegt mindestens ein Treffer im Radius, bleiben die Treffer im Radius und alle Orte, in der
   bisherigen Reihenfolge.
2. Liegt kein Treffer im Radius, zeigt die App die nächsten Treffer von weiter weg, auch POIs und
   Straßen. Ein ferner Ort mit ähnlichem Namen verdeckt so kein gesuchtes Hostel.
3. Der Hinweis „Nichts im Umkreis, nächste Treffer weiter entfernt.“ erscheint nur in Fall 2 und
   nur, wenn kein Ort den Suchbegriff als ganzes Wort enthält. „Cusco“ von zu Hause zeigt also
   einfach Cusco.

**Kategorieknöpfe.** Streng im Radius. Ohne Treffer: „Keine Treffer im Umkreis. Zoome heraus.“

**Punkte.** Jeder Treffer ist ein kleiner Punkt. Der gewählte Treffer behält den roten Pin, die
anderen bleiben sichtbar. Ein Klick auf einen Punkt wirkt wie ein Klick in der Liste. Der Mauszeiger
zeigt, dass der Punkt anklickbar ist. Während Wegpunkte gesetzt werden, reagieren die Punkte nicht
auf Klicks. Nach „Route hierher“ und beim Leeren der Suche verschwinden sie.

**Einpassen.** Nach einer Kategorie sofort, beim Tippen erst nach einer kurzen Pause (etwa 500 ms
nach dem Ergebnis). Eingepasst wird auf die Treffer im Radius. Gibt es dort keine, auf alle. So zieht
ein einzelner weit entfernter Ort die Karte nicht weit heraus. Ein einzelner Treffer wird wie bisher
angeflogen.

**Zurück.** Die Karte des gewählten Treffers bekommt einen Knopf „Zurück“. Er stellt Trefferliste,
Hinweise und den Kartenausschnitt von vor der Auswahl wieder her, entfernt den Pin und beendet
eine angefangene „Route hierher“. Es wird nicht neu gesucht. Suchtext und gewählte Kategorie
bleiben stehen.

## Umsetzung

**Suche unter der Haube (Entscheidung Claude).** Der Nutzer hat das überlassen.

- Text: Die Suche prüft schon heute jeden Namenstreffer und berechnet dessen Entfernung. Der
  Umkreis wird danach in `core-app` angewendet. Weil Treffer nach Rang und Entfernung sortiert
  sind, kostet das praktisch nichts und braucht keine Änderung am Index.
- Kategorien: `by_category` nutzt den vorhandenen Index auf Kategorie und Breitengrad. Es bekommt
  eine Obergrenze und hört auf zu suchen, sobald das Band den Radius abdeckt. Das macht seltene
  Kategorien schneller als heute.

**Schnittstellen.** `App::search` und `App::search_category` bekommen `radius_m` und begrenzen ihn
selbst auf 2 bis 50 km, damit Base und Trail dieselbe Regel haben. `SearchResults` bekommt die
Felder `widened` und `radius_m` (der tatsächlich verwendete Radius). Die Tauri-Kommandos und
`core-ffi` reichen sie durch. Nach der Kernänderung muss auf
dem Mac `apps/trail/scripts/build-core.sh` laufen.

**Karte.** Die Punkte sind eine GeoJSON-Ebene in `map.ts`, keine einzelnen DOM-Marker, und werden
wie die anderen Ebenen beim Laden eingerichtet.

## Tests

- Rust: Radiusgrenzen, Orte außerhalb des Radius, `widened`, strenger Kategorieradius, mehrere
  Regionen.
- Base-Selbsttest mit echten Mausereignissen: suchen, Punkte prüfen, Punkt anklicken, „Zurück“,
  Liste und Ausschnitt prüfen, danach Route zu einem Treffer.
- Der Nutzer prüft das Verhalten von Hand.
