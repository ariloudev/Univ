#!/usr/bin/env python3
"""Prepare 2MPZ (TWOMPZ..twompzPhotoz) for Univ Ingest: read it from the SuperCOSMOS Science
Archive's freeform SQL form, one query at a time, in ranges of the integer key twomassID, and join
the ranges into twompz.csv.gz, the prepared file that sources/twompz.yaml names.

The form cancels answers of more than about 500 to 800 rows, so the range adapts: halved on a
cancellation, and after each answer sized from the density of rows just seen to hold about
TARGET_ROWS, never more than MAX_KEYS keys (about 5,000 rows, the owner's limit). Each range is
kept in its own file under OUT_DIR/parts, so a rerun continues where it stopped. At the end the
ranges are joined into OUT_DIR/twompz.csv.gz and its rows checked against the count, and its
SHA-256 printed. The gzip header holds no time or name, so the same rows give the same digest.
A file whose digest differs from the definition's is refused by univ-ingest: a new version of the
table needs the definition's sha256 and rows changed with it. (The file of 2026-11-05, which the
definition names, was written with the time in its header: a rerun gives other bytes from the
same rows, and the definition's sha256 changes with it.)

Usage, from the repository root: python3 -I scripts/fetch-twompz.py [OUT_DIR]
OUT_DIR defaults to Univ Ingest's download cache for the source, ../downloads/univ-ingest/twompz,
or $UNIV_INGEST_CACHE/twompz.
"""

import gzip
import hashlib
import io
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

FORM = "http://ssa.roe.ac.uk:8080/ssa/SSASQL"
AGENT = "univ-ingest/0.1 (sequential 2MPZ download, scripts/fetch-twompz.py)"
TABLE = "TWOMPZ..twompzPhotoz"
COLUMNS = "twomassID, twomassX, ra, dec, zPhoto, zSpec"
MAX_KEYS = 26000  # about 5,000 rows: 934,175 rows over keys 3 to 5,000,534
MIN_KEYS = 50
TARGET_ROWS = 350  # below the archive's limit of 500 to 800 rows a query
PAUSE = 2.0  # seconds between queries
EXPECTED = 934175  # COUNT(*) on 2026-11-05


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


def post(sql, timeout=600):
    form = urllib.parse.urlencode({
        "action": "freeform", "server": "amachine", "sqlstmt": sql, "emailAddress": "",
        "format": "CSV", "compress": "GZIP", "rows": "1",
    }).encode()
    req = urllib.request.Request(FORM, data=form, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read().decode("latin-1")


def get(url, timeout=600):
    req = urllib.request.Request(url, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def query(sql):
    """('ok', csv_text) | ('cancelled', why) | ('failed', why)."""
    try:
        page = post(sql)
    except (urllib.error.URLError, TimeoutError, ConnectionError, OSError) as e:
        return "failed", f"post: {e}"
    if "SQL Error" in page:
        why = re.sub(r"<[^>]*>", " ", page.split("SQL Error", 1)[1])[:160].strip()
        kind = "cancelled" if "cancelled" in why or "timeout" in why.lower() else "failed"
        return kind, why
    link = re.search(r'href="([^"]+\.csv\.gz)"', page)
    count = re.search(r"\(Contains\s*(?:<b>)?\s*(\d+)\s*rows?\b", page)
    if not link or not count:
        return "failed", "page without results link or row count"
    try:
        text = gzip.decompress(get(link.group(1))).decode("latin-1")
    except (urllib.error.URLError, TimeoutError, ConnectionError, OSError, EOFError) as e:
        return "failed", f"file: {e}"
    rows = data_lines(text)
    if rows != int(count.group(1)):
        return "failed", f"file holds {rows} rows, page says {count.group(1)}"
    return "ok", text


def data_lines(text):
    """The rows of a CSV answer: its lines after the header that are not blank."""
    return sum(1 for l in text.split("\n")[1:] if l.strip())


def scalar(sql):
    for attempt in range(5):
        kind, out = query(sql)
        if kind == "ok":
            return int(out.split("\n")[1].strip())
        log(f"{sql!r}: {kind}: {out}")
        time.sleep(10 * (attempt + 1))
    sys.exit(f"could not run {sql!r}")


def main(directory):
    parts = os.path.join(directory, "parts")
    os.makedirs(parts, exist_ok=True)
    lo_key = scalar(f"SELECT MIN(twomassID) AS n FROM {TABLE}")
    hi_key = scalar(f"SELECT MAX(twomassID) AS n FROM {TABLE}")
    log(f"twomassID from {lo_key} to {hi_key}")
    done = {}
    for name in os.listdir(parts):
        m = re.fullmatch(r"(\d+)-(\d+)\.csv", name)
        if m:
            done[int(m.group(1))] = int(m.group(2))
    start, step, failures, rows = lo_key, 500, 0, 0
    while start <= hi_key:
        if start in done:
            start = done[start] + 1
            continue
        end = min(start + step - 1, hi_key)
        sql = (f"SELECT {COLUMNS} FROM {TABLE} "
               f"WHERE twomassID >= {start} AND twomassID <= {end}")
        kind, out = query(sql)
        time.sleep(PAUSE)
        if kind == "ok":
            n = data_lines(out)
            path = os.path.join(parts, f"{start}-{end}.csv")
            with open(path + ".part", "w") as f:
                f.write(out)
            os.replace(path + ".part", path)
            rows += n
            log(f"{start}-{end}: {n} rows (step {step}, {rows} this run, "
                f"{100 * (end - lo_key) / (hi_key - lo_key):.1f}% of keys)")
            # The archive cancels answers of more than about 500 to 800 rows (TOP 500 answered,
            # TOP 800 was cancelled, 2026-11-05): size the next range from the density just seen.
            density = max(n, 1) / (end - start + 1)
            step = max(MIN_KEYS, min(MAX_KEYS, step * 2, int(TARGET_ROWS / density)))
            start, failures = end + 1, 0
        elif kind == "cancelled" and step > MIN_KEYS:
            step = max(MIN_KEYS, step // 2)
            log(f"{start}-{end}: cancelled, step now {step}")
        else:
            failures += 1
            log(f"{start}-{end}: {kind}: {out} (failure {failures})")
            if failures >= 8:
                sys.exit("too many failures in a row; rerun to continue")
            time.sleep(min(300, 15 * failures))
    join(directory, parts)


def join(out, parts):
    names = sorted(
        (n for n in os.listdir(parts) if n.endswith(".csv")),
        key=lambda n: int(n.split("-")[0]),
    )
    header, total, ids = None, 0, set()
    path = os.path.join(out, "twompz.csv.gz")
    # No time and no name in the gzip header, so that the same rows give the same bytes.
    with open(path + ".part", "wb") as raw, gzip.GzipFile(
        filename="", mode="wb", fileobj=raw, mtime=0
    ) as packed, io.TextIOWrapper(packed, encoding="utf-8", newline="") as z:
        for name in names:
            with open(os.path.join(parts, name)) as f:
                lines = f.read().split("\n")
            if header is None:
                header = lines[0]
                z.write(header + "\n")
            elif lines[0] != header:
                sys.exit(f"{name}: header differs")
            for line in lines[1:]:
                if line.strip():
                    key = line.split(",")[0].strip()
                    if key in ids:
                        sys.exit(f"{name}: twomassID {key} twice")
                    ids.add(key)
                    z.write(line + "\n")
                    total += 1
    os.replace(path + ".part", path)
    digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
    log(f"joined {len(names)} parts: {total} rows (expected {EXPECTED}); sha256 {digest}")
    if total != EXPECTED:
        sys.exit(f"INCOMPLETE: {total} rows, the archive counts {EXPECTED}")
    print("COMPLETE", flush=True)


def default_out():
    cache = os.environ.get("UNIV_INGEST_CACHE", os.path.join("..", "downloads", "univ-ingest"))
    return os.path.join(cache, "twompz")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else default_out())
