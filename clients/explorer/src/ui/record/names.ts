// Short names for the record panels (read-an-object, see-the-structures): a source by the name
// astronomers call it, its title where the explorer knows none, and a paper by its first author
// and year.

/** Each source of the release by its short name. Release 13 is to carry these in the sources'
 * own definitions; until then the explorer keeps them, and a source it does not know is named
 * by its title. */
const SHORT: Readonly<Record<string, string>> = {
  "alfalfa-a100": "ALFALFA α.100",
  "bailer-jones-2021": "Bailer-Jones 2021",
  "boss-dr17-great-wall": "BOSS DR17",
  "cf4-basins": "CF4 basins",
  "cf4pp-density": "CF4++",
  "cosmicflows-4-galaxies": "Cosmicflows-4",
  "cosmicflows-4": "Cosmicflows-4 groups",
  "desi-dr1": "DESI DR1",
  "erass1-clusters": "eRASS1",
  "gaia-dr3-bright": "Gaia DR3",
  "gaia-dr3-hipparcos2": "Gaia DR3–Hipparcos-2",
  "gama-dr4": "GAMA DR4",
  gcns: "GCNS",
  "hipparcos-1997": "Hipparcos 1997",
  "hipparcos-2007": "Hipparcos 2007",
  "iau-star-names": "IAU WGSN",
  "mcconnachie-2012": "McConnachie 2012",
  openngc: "OpenNGC",
  "researched-star-distances": "Researched distances",
  "researched-structures": "Researched structures",
  "sdss-dr17-distant": "SDSS DR17, z > 0.35",
  "sdss-dr17-galaxies": "SDSS DR17",
  "sixdfgs-dr3": "6dFGS DR3",
  twodfgrs: "2dFGRS",
  "twompp-catalogue": "2M++",
  "twompp-reconstruction": "2M++ reconstruction",
  twompz: "2MPZ",
  twomrs: "2MRS",
  "updated-nearby-galaxies": "Karachentsev 2013",
  "wise-supercosmos": "WISE × SuperCOSMOS",
};

/** Source `id` by its short name, or by `title`, or by its id where neither is known. */
export const sourceShort = (id: string, title?: string): string => SHORT[id] ?? title ?? id;

/** A paper's reference split into its first author and year, and the rest after the year. */
export interface PaperName {
  short: string;
  rest: string;
}

/** A reference such as "Kraan-Korteweg, Cluver & Chon 2017, MNRAS Letters 466, L29" as its first
 * author and year, "Kraan-Korteweg et al. 2017", and the rest, "MNRAS Letters 466, L29". */
export function paperShort(reference: string): PaperName {
  const found = /^(.*?),? (\d{4}[a-z]?)\b,?\s*(.*)$/.exec(reference);
  if (found === null) return { short: reference, rest: "" };
  const [, authors = "", year = "", rest = ""] = found;
  const names = authors.split(/, | & /).filter((n) => n.trim() !== "");
  const first = names[0] ?? authors;
  return { short: `${first}${names.length > 1 ? " et al." : ""} ${year}`, rest };
}
