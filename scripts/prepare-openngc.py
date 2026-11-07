#!/usr/bin/env python3
"""Prepare OpenNGC (Mattia Verga, CC BY-SA 4.0) for Univ Ingest: read the two database files of
release v20260501 from GitHub, check them against the SHA-256 of the files read on 2026-11-07, and
write openngc.csv.gz, the prepared file that sources/openngc.yaml names.

OpenNGC writes its files as semicolon-separated tables, with sexagesimal positions, zero-padded
names (NGC0224) and other catalogues' designations listed together in one cell. The prepared file
keeps every row of both files, Dup and NonEx included (the definition leaves them out with their
reasons), and writes each in plain comma-separated columns:

  name          the name without zero padding: NGC0224 as NGC 224, IC0001 as IC 1, UGC04305 as
                UGC 4305 (catalogue letters, a space, the number, any suffix as published)
  type          OpenNGC's object type code, as published
  ra_deg        RA in degrees, from the J2000 sexagesimal RA (hours)
  dec_deg       Dec in degrees, from the J2000 sexagesimal Dec
  M             the Messier number as "M 31", from M "031"; blank where there is none
  PGC           the PGC number of the PGC designation in Identifiers, as published ("002557")
  2MASX         the 2MASX designation in Identifiers without "2MASX J" ("00424433+4116074")
  common_names  the Common names cell, as published, names separated by commas

Nothing else is taken: OpenNGC's sizes, magnitudes, redshifts and velocities, compiled from NED,
HyperLEDA and SIMBAD, are not read. The gzip header holds no time or name, so the same rows give
the same digest.

Usage, from the repository root: python3 -I scripts/prepare-openngc.py [OUT_DIR]
OUT_DIR defaults to Univ Ingest's download cache for the source, ../downloads/univ-ingest/openngc,
or $UNIV_INGEST_CACHE/openngc.
"""

import csv
import gzip
import hashlib
import io
import os
import re
import sys
import urllib.request

RELEASE = "v20260501"
BASE = f"https://raw.githubusercontent.com/mattiaverga/OpenNGC/{RELEASE}/database_files/"
FILES = {
    "NGC.csv": "840fe0c9ee1332e551b2e722a0e92726cd7b157914a3d2177602832aadd3aa9e",
    "addendum.csv": "1d8f0914e643ada325a5a94d88d8fefad6a4937a2f77cc34f21483af22b11983",
}
AGENT = "univ-ingest/0.1 (OpenNGC release download, scripts/prepare-openngc.py)"
COLUMNS = ["name", "type", "ra_deg", "dec_deg", "M", "PGC", "2MASX", "common_names"]


def fetched(name):
    """The bytes of database file `name`, checked against its digest."""
    request = urllib.request.Request(BASE + name, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(request, timeout=120) as answer:
        body = answer.read()
    digest = hashlib.sha256(body).hexdigest()
    if digest != FILES[name]:
        sys.exit(f"{name}: SHA-256 {digest}, not the {FILES[name]} of release {RELEASE}")
    return body


def unpadded(name):
    """`name` with its catalogue letters, a space, and its number without leading zeros."""
    m = re.fullmatch(r"([A-Za-z]+)0*(\d+)(.*)", name)
    return f"{m.group(1)} {m.group(2)}{m.group(3)}" if m else name


def degrees(text, scale):
    """Sexagesimal `text` (h:m:s or d:m:s) in degrees, times `scale`; blank for a blank cell."""
    if not text:
        return ""
    sign = -1.0 if text.startswith("-") else 1.0
    parts = [float(p) for p in text.lstrip("+-").split(":")]
    if len(parts) != 3:
        sys.exit(f"{text!r} is not sexagesimal")
    value = sign * scale * (parts[0] + parts[1] / 60 + parts[2] / 3600)
    return f"{value:.7f}"


def designation(identifiers, prefix):
    """The designation of `identifiers` that starts with `prefix`, without it; blank if none."""
    found = [i[len(prefix) :] for i in identifiers.split(",") if i.startswith(prefix)]
    return found[0] if found else ""


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else None
    if out_dir is None:
        cache = os.environ.get("UNIV_INGEST_CACHE", "../downloads/univ-ingest")
        out_dir = os.path.join(cache, "openngc")
    rows = []
    for name in FILES:
        text = fetched(name).decode("utf-8")
        rows.extend(csv.DictReader(io.StringIO(text), delimiter=";"))
    out = io.StringIO()
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(COLUMNS)
    for r in rows:
        messier = str(int(r["M"])) if r["M"] else ""
        writer.writerow(
            [
                unpadded(r["Name"]),
                r["Type"],
                degrees(r["RA"], 15.0),
                degrees(r["Dec"], 1.0),
                f"M {messier}" if messier else "",
                designation(r["Identifiers"], "PGC "),
                designation(r["Identifiers"], "2MASX J"),
                r["Common names"],
            ]
        )
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, "openngc.csv.gz")
    with open(path, "wb") as f:
        with gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=0) as gz:
            gz.write(out.getvalue().encode("utf-8"))
    digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
    print(f"{len(rows)} rows written to {path}")
    print(f"sha256: {digest}")


if __name__ == "__main__":
    main()
