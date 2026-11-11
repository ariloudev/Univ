#!/usr/bin/env python3
"""Prepare the Galactic globular clusters of Holger Baumgardt's database for Univ Ingest: read its
orbits table, check it against the SHA-256 of the file read on 2026-11-10, and write
baumgardt-vasiliev-2021.csv.gz, the prepared file that sources/baumgardt-vasiliev-2021.yaml names.

The table (people.smp.uq.edu.au/HolgerBaumgardt/globular/orbits_table.txt) is whitespace-aligned
text with three '#' header lines and 31 columns a row. The database's page says its version "uses
... the cluster distances derived in Baumgardt & Vasiliev (2021)". The prepared file keeps every
row, and of each the columns below, as published:

  name          (1) Cluster name, as published, such as NGC_104
  ra_deg        (2) RA, degrees
  dec_deg       (3) DEC, degrees
  rsun_kpc      (6) Rsun, the distance from the Sun, kpc
  rsun_err_kpc  (7) Delta_R, its error, kpc
  pmra          (11) mualpha, the proper motion in RA (times cos DEC), mas/yr
  pmdec         (13) mu_delta, the proper motion in DEC, mas/yr

Nothing is computed. The gzip header holds no time or name, so the same rows give the same
digest.

Usage, from the repository root: python3 -I scripts/prepare-baumgardt-globulars.py [OUT_DIR]
OUT_DIR defaults to Univ Ingest's download cache for the source,
../downloads/univ-ingest/baumgardt-vasiliev-2021, or $UNIV_INGEST_CACHE/baumgardt-vasiliev-2021.
"""

import csv
import gzip
import hashlib
import io
import os
import sys
import urllib.request

URL = "https://people.smp.uq.edu.au/HolgerBaumgardt/globular/orbits_table.txt"
SHA256 = "c95a085586e11e29a655a69fb9c31116072f774209ce561c11c92de1ede4bda3"
AGENT = "univ-ingest/0.1 (globular cluster table download, scripts/prepare-baumgardt-globulars.py)"
# The 0-based place in a row of each column taken, and its name in the prepared file.
TAKEN = [(0, "name"), (1, "ra_deg"), (2, "dec_deg"), (5, "rsun_kpc"), (6, "rsun_err_kpc"),
         (10, "pmra"), (12, "pmdec")]
FIELDS = 31


def fetched():
    """The bytes of the orbits table, checked against its digest."""
    request = urllib.request.Request(URL, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(request, timeout=120) as answer:
        body = answer.read()
    digest = hashlib.sha256(body).hexdigest()
    if digest != SHA256:
        sys.exit(f"orbits_table.txt: SHA-256 {digest}, not the {SHA256} read on 2026-11-10")
    return body


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else None
    if out_dir is None:
        cache = os.environ.get("UNIV_INGEST_CACHE", "../downloads/univ-ingest")
        out_dir = os.path.join(cache, "baumgardt-vasiliev-2021")
    out = io.StringIO()
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow([name for _, name in TAKEN])
    rows = 0
    for line in fetched().decode("utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split()
        if len(cells) != FIELDS:
            sys.exit(f"{len(cells)} columns, not {FIELDS}: {line!r}")
        writer.writerow([cells[i] for i, _ in TAKEN])
        rows += 1
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, "baumgardt-vasiliev-2021.csv.gz")
    with open(path, "wb") as f:
        with gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=0) as gz:
            gz.write(out.getvalue().encode("utf-8"))
    digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
    print(f"{rows} rows written to {path}")
    print(f"sha256: {digest}")


if __name__ == "__main__":
    main()
