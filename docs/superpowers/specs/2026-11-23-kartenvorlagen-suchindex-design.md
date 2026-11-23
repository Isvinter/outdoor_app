# Kartenvorlagen und Suchindex (Map-Builder, Schritt 1)

Stand 2026-11-23. Erster von vier Schritten zum Map-Builder in ZeroGrid Base.

## Absicht

ZeroGrid soll ein eigenes System sein, das keine Daten nach außen sendet und auch nach einem Jahr ohne Internet noch funktioniert. Internet ist nur beim bewussten Bauen von Karten in Base nötig.

Bisher kann Base nur eine Art Paket bauen, das auf Wandern, Rad und Skitour ausgelegt ist. Künftig soll es auch eine Karte für Auto und normalen Urlaub geben, als Ersatz für Google Maps. Der typische Fall: Man kommt spät in einer unbekannten Stadt an und muss ohne Netz das Hostel oder seine Straße finden.

Schritt 1 schafft die Grundlage: Das Bauskript kennt Vorlagen und legt einen Offline-Suchindex ins Paket. Alle Apps kommen mit den neuen Paketen zurecht.

## Gesamtbild

1. **Kartenvorlagen und Suchindex (dieses Dokument).**
2. Suche und „Route hierher“ in Base und Trail.
3. Builder-Oberfläche in Base mit schlankem Navigationsgerüst, Vorlagenwahl, Größe und Datenqualität.
4. Bibliothek vorgebauter Regionen auf einem Always-on-Server, von dem die mobilen Apps unterwegs Karten holen.

Danach: Bauzeit, restliche Base-UI, Sky-UI. Sprachansagen und Hausnummern vielleicht später.

## Ziele

- Drei Vorlagen: **Urlaub & Auto**, **Outdoor komplett**, **Fliegen**.
- Jedes neu gebaute Paket enthält eine Suchdatei mit Orten, Straßennamen und POIs.
- Base, Trail und Sky installieren, teilen, zeigen und löschen Pakete aller Vorlagen.
- Vorhandene Pakete laufen unverändert weiter, nur ohne Suche.

## Nicht in Schritt 1

- Keine Suchoberfläche und kein „Route hierher“ (Schritt 2).
- Keine Vorlagenwahl in der Oberfläche von Base (Schritt 3). Vorlagen werden bis dahin in der Regionsdefinition gesetzt.
- Kein Server (Schritt 4).
- Keine Hausnummern, keine Adresssuche mit Hausnummer.
- Kein Overture oder andere Zusatzquellen. POIs kommen nur aus OpenStreetMap.
- Kein Nachrüsten von Ebenen. Pakete sind fest, eine andere Auswahl bedeutet einen neuen Bau.
- Keine Optimierung der Bauzeit.

## Das darf nicht passieren

- Eine Funktion hängt im Betrieb von Netz, Konto oder einem Online-Dienst ab.
- Ein vorhandenes Paket oder eine vorhandene Region wird unbrauchbar.
- Sky stellt Luftraumhöhen über Grund wegen fehlender Geländedaten günstiger dar als bisher. Ohne Gelände bleibt es bei der heutigen Annahme des ungünstigsten Falls.

## Entscheidungen des Nutzers

- Vorlagen plus wählbare Ebenen, kein freier Stil-Editor.
- Pakete sind fest (kein Nachrüsten). Karten werden vor der Reise vorbereitet.
- Karten werden ausschließlich in Base gebaut.
- Suche nach Orten, POIs und Straßennamen. Hausnummern sind nicht nötig.
- Gewünschte POIs: Tankstellen, Supermärkte, Restaurants, Apotheken, Unterkünfte, Sehenswürdigkeiten, Parkplätze, Campingplätze und jede Art von Einzelhandel.
- Das Paketformat wird behutsam erweitert (Ansatz 1 unten).
- Dieselbe Region darf in mehreren Vorlagen existieren. Jede Vorlage bekommt einen eigenen Paketnamen.
- Reihenfolge der vier Schritte wie oben.

## Ansätze für die Suchdatei im Paket

1. **Format 1 behutsam erweitern (gewählt).** `search.sqlite` wird eine normale Datei im Manifest. Download, Prüfsumme, Fortsetzen, Teilen und Löschen funktionieren dadurch ohne eigenen Code. Nachteil: Eine nicht aktualisierte App lehnt Pakete mit Suchdatei ab. Weil der Nutzer alle Apps selbst aktualisiert, ist das hinnehmbar.
2. **Formatversion 2.** Alte Apps lehnen dann jedes neue Paket ab, neue Apps müssen dauerhaft zwei Formate lesen. Mehr Aufwand ohne Nutzen.
3. **Suchdatei neben dem Paket.** Alte Apps merken nichts, aber Download, Prüfung, Teilen und Löschen wären doppelt zu bauen, und Karte und Suche könnten auseinanderlaufen.

## Entwurf

### Vorlagen

Die Regionsdefinition bekommt das optionale Feld `template` mit den Werten `urlaub`, `outdoor` oder `fliegen`. Fehlt es, gilt `outdoor`. Bestehende Definitionen bleiben damit gültig. Ein unbekannter Wert bricht den Bau mit klarer Meldung ab.

| Vorlage | Dateien im Paket | Bauschritte |
|---|---|---|
| `urlaub` | basemap, search, routing | osm, basemap, search, routing, manifest |
| `outdoor` | basemap, outdoor, dem, slope, contours, search, routing | osm, basemap, outdoor, dem, slope, contours, search, routing, manifest |
| `fliegen` | basemap, outdoor, dem, contours, search | osm, basemap, outdoor, dem, contours, search, manifest |

`scripts/build-region.sh` leitet die Standardschritte aus der Vorlage ab. Ein ausdrückliches `--steps` bleibt möglich. Der Manifest-Schritt listet **nur die Dateien der Vorlage**, auch wenn im Ausgabeordner von einem früheren Bau weitere liegen. Der Schritt `search` wird wie `routing` bei `--force` und `--refresh-osm` neu gebaut.

Base baut weiter ohne Vorlagenwahl, also `outdoor`, und bekommt damit automatisch die Suche. Die Fortschrittsanzeige braucht eine Beschriftung für `search`. Auch für den vorhandenen Schritt `contours` fehlt sie und wird ergänzt.

### Paketnamen je Vorlage

Ein Paket wird weiterhin allein über seine `id` erkannt. Speicher, Installation, Freigabe und Updates bleiben dadurch unverändert.

- `outdoor` behält die Region als id, zum Beispiel `alpenraum-ost`. Vorhandene Pakete bleiben so gültig und bekommen weiter ihre Updates.
- `urlaub` und `fliegen` hängen die Vorlage an: `alpenraum-ost-urlaub`, `alpenraum-ost-fliegen`.
- `build-region.sh` bricht **vor dem ersten Schritt** ab, wenn id und Vorlage nicht zusammenpassen: Eine `urlaub`-Definition muss auf `-urlaub` enden, eine `fliegen`-Definition auf `-fliegen`, eine `outdoor`-Definition darf auf keins von beiden enden. So kann ein Bau nie ein Paket einer anderen Vorlage überschreiben.
- Manifest und Katalog bekommen das optionale Feld `group` mit der id der Region ohne Vorlagen-Endung. Schritt 3 kann Varianten derselben Region damit in der Liste zusammenfassen. Fehlt das Feld, gilt die eigene id.

Liegen mehrere Varianten auf einem Gerät, funktioniert die Karte weiter: Kacheln und Höhen kommen aus dem Paket, das sie hat, und Routing aus dem ersten passenden Paket mit Routingdaten. Basiskarte und Routing belegen dann doppelten Speicher. Die Apps warnen in Schritt 1 nicht davor.

### Paketformat

- `FORMAT` bleibt 1.
- `RegionManifest::parse` akzeptiert zusätzlich genau den Dateinamen `search.sqlite` (neue Konstante neben `ROUTING_FILE`). Andere Dateitypen bleiben verboten.
- Manifest und Katalog bekommen ein optionales Feld `template`. Fehlt es, gilt das Paket als `outdoor`. Der von Base erzeugte Freigabe-Katalog gibt es weiter.
- Das Manifest bekommt optional `search` mit `format`, `places`, `streets`, `pois` und `named_pois`. Schritt 3 nutzt diese Zahlen für die Anzeige der Datenqualität.
- `load_region`, Installation, Freigabeserver und Löschen brauchen nach heutigem Code keine eigene Änderung. Tests müssen das für `search.sqlite` belegen.

**Reihenfolge beim Ausrollen:** Erst Base, Trail und Sky aktualisieren, dann Pakete mit Suchdatei verteilen.

### Suchdatei

Eine SQLite-Datei mit FTS5 je Paket, nur lesend genutzt. SQLite ist über `rusqlite` (gebündelt) schon im Projekt. Dass FTS5 auf allen Zielplattformen, auch iOS, in diesem Build aktiv ist, wird als erste Aufgabe geprüft.

Eine neue Bibliothek `crates/core-search` enthält Schema, Normalisierung und eine einfache Abfrage. Ein neues Werkzeug `tools/search-index` schreibt die Datei. So nutzen Bau und spätere Suche dieselbe Normalisierung.

Inhalt je Eintrag:

- Art: `place`, `street` oder `poi`
- Kategorie und OSM-Untertyp (zum Beispiel `shop=bakery`)
- Anzeigename, zusätzliche Suchnamen und normalisierter Suchtext
- zugehöriger Ort für die Anzeige, zum Beispiel „Hauptstraße, Kufstein“
- Koordinate

Eine Tabelle `meta` hält mindestens Versionsnummer des Suchformats und Stand der OSM-Daten.

**Quellen:** Der Schritt liest `region.osm.pbf` und nutzt das vorhandene osmium-Image. Neue Programme braucht der Bau nicht.

**Orte:** `place=city, town, village, suburb, quarter, neighbourhood, hamlet`.

**Straßen:** benannte Wege mit `highway=motorway, trunk, primary, secondary, tertiary, unclassified, residential, living_street, pedestrian, service, road`. Abschnitte derselben Straße werden zusammengefasst, damit eine Straße je Ort nur einmal erscheint.

**POI-Kategorien:**

| Kategorie | OSM |
|---|---|
| Tankstellen | `amenity=fuel` |
| Supermärkte und Lebensmittel | `shop=supermarket, convenience, discount, greengrocer, grocery` |
| Gastronomie | `amenity=restaurant, cafe, fast_food, bar, pub, biergarten` |
| Apotheken | `amenity=pharmacy` |
| Unterkünfte | `tourism=hotel, hostel, guest_house, motel, apartment, chalet, alpine_hut` |
| Camping | `tourism=camp_site, caravan_site` |
| Parkplätze | `amenity=parking`, ohne `access=private` |
| Einzelhandel | jedes weitere `shop=*` |
| Sehenswürdigkeiten | `tourism=attraction, museum, viewpoint, gallery, zoo, theme_park`, `historic=castle, monument, ruins, archaeological_site, fort, city_gate` |

POIs ohne Namen bleiben enthalten, damit sie über die Kategorie auffindbar sind („Parkplatz in der Nähe“).

**Namen:** Gesucht wird in `name`, `name:de`, `name:en`, `int_name`, `alt_name` und `short_name`. So ist Bozen auch als Bolzano zu finden.

**Normalisierung:** Klein geschrieben, ohne Akzente, `ß` wird `ss`. Deutsche Umlaute sind sowohl als `a` als auch als `ae` auffindbar, `Str.` und `Straße` gelten als gleich. Die Suche passt auch auf Wortanfänge.

**Zugehöriger Ort:** der nächstgelegene passende `place`-Punkt, mit größerem Suchradius für größere Orte. Das ist eine Näherung und darf an Gemeindegrenzen danebenliegen. Findet sich kein Ort, bleibt das Feld leer.

### Pakete ohne Gelände

- Karten: Fehlende Kacheln liefern die Apps schon heute als leer. Base, Trail und Sky müssen eine Urlaubskarte ohne Fehler und ohne Dauerlast durch vergebliche Anfragen zeigen.
- Routen: Ohne Geländedaten bleiben Anstieg und Abstieg leer, und Base hat dafür schon einen Hinweis. Trail darf kein leeres Höhendiagramm zeigen, sondern denselben Hinweis.
- Sky: Die Luftraumwarnung nimmt ohne Gelände wie bisher den ungünstigsten Fall an. Ein Test belegt das.
- Relief- und Hangneigungsregler bleiben in Schritt 1 sichtbar, auch wenn sie bei einer Urlaubskarte nichts bewirken. Das klärt die jeweilige UI-Session.

## Abnahme

- Bestehende Tests laufen, vorhandene Pakete lassen sich weiter installieren, teilen und anzeigen.
- Das Gebiet von `alpenraum-ost` wird als neues Paket `alpenraum-ost-urlaub` gebaut, das installierte `alpenraum-ost` bleibt dabei unverändert. Paket höchstens 2,6 GB, `search.sqlite` höchstens 100 MB. Bei Überschreitung anhalten und mit dem Nutzer sprechen.
- Ein Bau mit unpassender Kombination aus id und Vorlage bricht ab, bevor er etwas schreibt.
- Über die Abfrage von `core-search` werden in `alpenraum-ost-urlaub` ein Hostel per Name, eine Straße mit Ort, „Bozen“ und „Bolzano“ sowie eine Apotheke über ihre Kategorie gefunden.
- Ein Urlaubspaket läuft in Base, Trail und Sky: Installation über die Freigabe, Karte, Autoroute, Löschen.
- Eine kleine Testregion um Cusco wird als `urlaub` gebaut. POI-, Straßen- und Ortszahlen kommen in `docs/MESSPROTOKOLL.md`, weil die OSM-Abdeckung dort nicht angenommen werden soll.
- Die Größen je Vorlage stehen in `docs/MESSPROTOKOLL.md`.

## Bekannte Grenzen

- Routen gehen weiterhin nur innerhalb einer Region. Das wird für „Route hierher“ in Schritt 2 relevant.
- Die Daten stammen aus OSM und stehen unter ODbL. Die Attribution im Paket bleibt. Sollte die Bibliothek je öffentlich angeboten werden, sind die Lizenzpflichten für die abgeleitete Datenbank vorher zu prüfen.

## Dem Bauenden überlassen

- Genaues SQL-Schema und FTS5-Einstellungen, solange die Größengrenze hält.
- Radien für die Ortszuordnung und das Zusammenfassen von Straßenabschnitten.
- Aufteilung auf Aufgaben und Tests.
