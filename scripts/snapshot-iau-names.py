#!/usr/bin/env python3
"""Snapshot the IAU Catalog of Star Names (CSN) of the IAU Working Group on Star Names (WGSN) into
names/iau-wgsn.yaml, which sources/iau-star-names.yaml reads.

The list is published only as an HTML table, at https://exopla.net/star-names/modern-iau-star-names/,
the WGSN's own site. The script reads a saved copy of that page: save it first, for example with
    curl -sSfL -A "univ-ingest/0.1" -o page.html https://exopla.net/star-names/modern-iau-star-names/

Each star keeps its name, its designation (an HR, HD, Gliese or other catalogue designation), its
HIP number where it has one, its date of adoption, and the RA and Dec in degrees of the page's
hidden columns, which the page says come from the WGSN's Naked Eye star Catalog (NEC), compiled
from SIMBAD data. The etymology, language and image columns are not taken.

Usage, from the repository root:
    python3 -I scripts/snapshot-iau-names.py PAGE.html READ_ON [OUT]
READ_ON is the date the page was read, YYYY-MM-DD. OUT defaults to names/iau-wgsn.yaml.
"""

import html
import json
import re
import sys

PAGE = "https://exopla.net/star-names/modern-iau-star-names/"
HEADER = [
    "proper names",
    "NEC+",
    "Designation",
    "HIP",
    "Bayer ID",
    "Simbad spelling",
    "Constellation",
    "Origin",
    "Language",
    "Reference",
    "Date of Adoption",
    "image",
    "image_source",
    "RA",
    "DEC",
    "mag",
    "band",
]


def cells(row):
    """The text of each cell of an HTML table row."""
    raw = re.findall(r"<t[hd][^>]*>(.*?)</t[hd]>", row, re.S)
    return [html.unescape(re.sub(r"<.*?>", "", c)).strip() for c in raw]


def quoted(text):
    """`text` as a YAML double-quoted scalar: a JSON string is one, escapes and all."""
    return json.dumps(text, ensure_ascii=False)


def main():
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    page, read_on = sys.argv[1], sys.argv[2]
    out = sys.argv[3] if len(sys.argv) == 4 else "names/iau-wgsn.yaml"
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", read_on):
        sys.exit(f"READ_ON must be YYYY-MM-DD, not {read_on!r}")
    text = open(page, encoding="utf-8").read()
    table = max(re.findall(r"<table.*?</table>", text, re.S), key=len)
    rows = [cells(r) for r in re.findall(r"<tr.*?</tr>", table, re.S)]
    if rows[0] != HEADER:
        sys.exit(f"the table's header is not the one this script reads: {rows[0]}")
    stars = [r for r in rows[1:] if r[0] and r[0] != HEADER[0]]
    names = [r[0] for r in stars]
    if len(set(names)) != len(names):
        sys.exit("a name appears twice")
    lines = [
        "# The IAU Catalog of Star Names (CSN) of the IAU Working Group on Star Names (WGSN), read",
        "# by scripts/snapshot-iau-names.py from the WGSN's table; a new reading is a new snapshot.",
        f"page: {quoted(PAGE)}",
        f"read_on: {read_on}",
        "positions: >-",
        "  RA and Dec in degrees, ICRS, from the table's hidden RA and DEC columns, which the page",
        "  says come from the WGSN's Naked Eye star Catalog (NEC), compiled from SIMBAD data.",
        "stars:",
    ]
    for r in stars:
        row = dict(zip(HEADER, r))
        adopted = row["Date of Adoption"].replace("/", "-")
        lines.append(f"  - name: {quoted(row['proper names'])}")
        if row["Designation"]:
            lines.append(f"    designation: {quoted(row['Designation'])}")
        if row["HIP"]:
            if not row["HIP"].isdigit():
                sys.exit(f"{row['proper names']}: HIP {row['HIP']!r} is not a number")
            lines.append(f"    hip: {row['HIP']}")
        for key, column in (("ra_deg", "RA"), ("dec_deg", "DEC")):
            if row[column]:
                float(row[column])
                lines.append(f"    {key}: {row[column]}")
        if re.fullmatch(r"\d{4}-\d{2}-\d{2}", adopted):
            lines.append(f"    adopted: {adopted}")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    with_hip = sum(1 for r in stars if r[HEADER.index("HIP")])
    print(f"{len(stars)} names, {with_hip} with a HIP number, written to {out}")


if __name__ == "__main__":
    main()
