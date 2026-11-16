#!/usr/bin/env python3
"""Wegwerfcode. Wertet die CSV-Protokolle des Ortung-Tests aus.

    python3 auswerten.py [--grundlinie 0.8] lauf-*.csv

Schreibt je Abschnitt (gleiche Stufe am Stück) eine Tabellenzeile in Markdown
und legt neben jede CSV eine GPX mit den behaltenen Punkten (zum Ansehen im Desktop).
"""
import argparse
import csv
import math
import statistics
import sys
from datetime import datetime
from pathlib import Path


def zeit(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def abstand(a, b):
    r = 6371000.0
    p1, p2 = math.radians(a[0]), math.radians(b[0])
    dp, dl = p2 - p1, math.radians(b[1] - a[1])
    h = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * r * math.asin(math.sqrt(h))


def quantil(xs, q):
    if not xs:
        return float("nan")
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(q * len(xs)))]


def abschnitte(zeilen):
    """Teilt in Abschnitte gleicher Stufe."""
    aktuell, stufe = [], None
    for z in zeilen:
        if z["stufe"] != stufe and aktuell:
            yield stufe, aktuell
            aktuell = []
        stufe = z["stufe"]
        aktuell.append(z)
    if aktuell:
        yield stufe, aktuell


def akku(zs):
    a = [(zeit(z["zeit"]), float(z["akku"])) for z in zs if z["typ"] == "A" and z["akku"] and float(z["akku"]) >= 0]
    if len(a) < 2:
        return None, None, None
    dauer = (a[-1][0] - a[0][0]).total_seconds() / 3600
    gesamt = (a[0][1] - a[-1][1]) * 100 / dauer if dauer > 0 else None
    # Zeitpunkte der 1-%-Schritte: genauer als Start/Ende.
    schritte = [a[i] for i in range(1, len(a)) if a[i][1] != a[i - 1][1]]
    genau = None
    if len(schritte) >= 2:
        h = (schritte[-1][0] - schritte[0][0]).total_seconds() / 3600
        if h > 0.2:
            genau = (schritte[0][1] - schritte[-1][1]) * 100 / h
    return (a[0][1] - a[-1][1]) * 100, gesamt, genau


def auswerten(pfad, grundlinie):
    with open(pfad, newline="") as f:
        zeilen = list(csv.DictReader(f))
    for stufe, zs in abschnitte(zeilen):
        t0, t1 = zeit(zs[0]["zeit"]), zeit(zs[-1]["zeit"])
        stunden = (t1 - t0).total_seconds() / 3600
        p = [z for z in zs if z["typ"] == "P" and z["behalten"] == "1"]
        alle = [z for z in zs if z["typ"] == "P"]
        wege, takte = [], []
        for a, b in zip(p, p[1:]):
            wege.append(abstand((float(a["lat"]), float(a["lon"])), (float(b["lat"]), float(b["lon"]))))
            takte.append((zeit(b["zeit"]) - zeit(a["zeit"])).total_seconds())
        luecken = [t for t in takte if t > 30]
        gen = [float(z["h_gen"]) for z in p]
        delta, gesamt, genau = akku(zs)
        e = [z["text"] for z in zs if z["typ"] == "E"]
        takt_luecken = sum(1 for t in e if t.startswith("Takt-Lücke"))
        pausen = sum(1 for t in e if "pausiert" in t and t.startswith("iOS hat"))
        netto = (genau if genau is not None else gesamt)
        netto = f"{netto - grundlinie:.2f}" if netto is not None and grundlinie else "–"
        fmt = lambda x, n=2: "–" if x is None or (isinstance(x, float) and math.isnan(x)) else f"{x:.{n}f}"
        print(f"| {Path(pfad).name} | {stufe} | {stunden:.2f} | {fmt(delta, 0)} | {fmt(gesamt)} | {fmt(genau)} | {netto} "
              f"| {len(p)} ({len(alle)}) | {fmt(sum(wege) / 1000)} | {fmt(statistics.median(wege) if wege else None, 1)} "
              f"| {fmt(statistics.median(takte) if takte else None, 1)} | {fmt(statistics.median(gen) if gen else None, 1)} "
              f"/ {fmt(quantil(gen, 0.9), 1)} | {len(luecken)} / {fmt(max(luecken) if luecken else None, 0)} "
              f"| {takt_luecken} | {pausen} |")
    gpx(pfad, [z for z in zeilen if z["typ"] == "P" and z["behalten"] == "1"])


def gpx(pfad, punkte):
    ziel = Path(pfad).with_suffix(".gpx")
    with open(ziel, "w") as f:
        f.write('<?xml version="1.0" encoding="UTF-8"?>\n<gpx version="1.1" creator="ortung-test" '
                'xmlns="http://www.topografix.com/GPX/1/1">\n<trk><name>%s</name>\n' % Path(pfad).stem)
        stufe = None
        for z in punkte:
            if z["stufe"] != stufe:
                if stufe is not None:
                    f.write("</trkseg>\n")
                f.write("<trkseg>\n")
                stufe = z["stufe"]
            f.write(f'<trkpt lat="{z["lat"]}" lon="{z["lon"]}"><ele>{z["hoehe"]}</ele><time>{z["zeit"]}</time></trkpt>\n')
        if stufe is not None:
            f.write("</trkseg>\n")
        f.write("</trk>\n</gpx>\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--grundlinie", type=float, default=0.0, help="%%/h der Leerlaufmessung")
    ap.add_argument("dateien", nargs="+")
    args = ap.parse_args()
    print("| Datei | Stufe | h | Akku −% | %/h gesamt | %/h Schritte | %/h netto | Punkte (alle) | km "
          "| Abstand m (Median) | Takt s (Median) | h_gen m Median / P90 | Lücken >30 s / max s | Takt-Lücken | iOS-Pausen |")
    print("| --- " * 15 + "|")
    for d in args.dateien:
        auswerten(d, args.grundlinie)


if __name__ == "__main__":
    sys.exit(main())
