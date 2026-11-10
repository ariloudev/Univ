# Univ

Real astronomical and cosmological data, normalized, cross-matched and served, with a 3D explorer
that draws it around Earth: from the nearest stars out through the Local Group, the galaxy
surveys and the cosmic web, to structures billions of light years away.

Every value comes from a published catalogue or paper, with its citation. What was measured is
kept apart from what was estimated or reconstructed, and uncertainties are kept rather than
dropped.

## The three products

| Product | What it is | Where |
|---|---|---|
| **Univ Ingest** | A local command line tool. It downloads public sky data, normalizes each catalogue into one model, cross-matches the entries into objects with stable ids, lays them out as tiles, and publishes immutable, numbered releases. | `crates/univ-ingest` |
| **Univ Server** | A Cloudflare Worker that serves the releases (objects, structures and tiles) and the layers clients write on top of them, with API keys and rate limits. It runs locally under `wrangler dev` for now. | `crates/univ-server`, `crates/univ-api` |
| **Univ Explorer** | A WebGL2 web app that draws a release in 3D around Earth. You turn, zoom and fly with the mouse, read any object's record and how its distance was derived, and browse the large-scale structures. | `clients/explorer` |

`crates/univ-model` holds the domain model the three share. `clients/tile-decoder` is the
reference TypeScript decoder of the tile format.

## The data

The repository holds the definitions of the data, not the data itself. Each source is a YAML file
in `sources/` that gives:

- how to fetch it and which cuts apply;
- the unit of each column and the field it maps to;
- the coordinate frame and epoch;
- the primary citation, the provider's terms, and the credit they require.

A release is built locally from those definitions. Downloads are kept outside the repository.

| Scale | Sources |
|---|---|
| Stars | Gaia DR3 (stars brighter than G = 10) and its Hipparcos-2 cross-match; the Gaia Catalogue of Nearby Stars; Hipparcos (ESA 1997, and van Leeuwen 2007); Bailer-Jones et al. 2021 distances; the IAU Catalog of Star Names |
| Nearby galaxies | McConnachie 2012; the Updated Nearby Galaxy Catalog (Karachentsev et al. 2013); OpenNGC |
| The cosmic web | 2MRS, 2M++ and 2MPZ; 6dFGS DR3; 2dFGRS; SDSS DR17 and the BOSS Great Wall region; DESI DR1; GAMA DR4; WISE × SuperCOSMOS; ALFALFA α.100; eRASS1 clusters; Cosmicflows-4 distances |
| Reconstructions | The CF4++ density and velocity fields, the Cosmicflows-4 basins of attraction, and the 2M++ density and velocity fields (Carrick et al. 2015) |

Beyond the catalogues, two hand-researched sets are read from papers:

- **Structures (`structures/`):** voids, walls, superclusters and arcs, such as the Boötes Void,
  the Local Void and the BOSS Great Wall. Each file cites its papers and says whether the
  structure is observed, a statistical claim, or reconstructed.
- **Star distances (`stars/`):** distances of named stars, each citing its papers.

The rules that turn all this into a release are in `rules/`:

- `matching.yaml`: cross-matching;
- `ranking.yaml`: which measurement is preferred;
- `designations.yaml`: names;
- `tiles.yaml`: the tile layout;
- `science-checks.yaml`: the cited checks each release must pass.

### Conventions

- **Cosmology:** distances derived from redshift use Planck 2018 unless a release names another.
  Every derived value records the steps and citations that produced it.
- **Frames:** galactic coordinates follow the Hipparcos catalogue (ESA 1997), and supergalactic
  coordinates follow Lahav et al. 2000.
- **Uncertainties:** a value published without an uncertainty is marked as having none reported,
  never as zero.

### Credit and terms

Anyone who uses a release must credit its sources as each provider asks. The credit lines are in
each `sources/<id>.yaml`. A source whose terms forbid redistribution is never published.

Gaia data are under CC BY-NC 3.0 IGO, so a release that includes Gaia is for non-commercial use.

## Running it

You need:

- Rust (stable);
- Node.js with pnpm 11;
- Wrangler, installed by `npm install` at the root.

```sh
npm install                      # Wrangler, which writes the local D1 and R2
cargo run -p univ-ingest -- help # fetch, normalize, match, tiles, build, publish, prune
```

`univ-ingest build` fetches every source, then builds the next release. `univ-ingest publish` puts
that release into the local D1 and R2 that the Worker reads. The full fetch downloads about 4.5
gigabytes, and some providers are slow.

Once a release is published, put an owner secret in `crates/univ-server/.dev.vars`, as
`OWNER_SECRET=...`. Git ignores that file. Then run:

```sh
cargo xtask explore
```

This starts Univ Server and the explorer together. Open http://127.0.0.1:5317/. The explorer's
own [README](clients/explorer/README.md) covers its controls and how to point it at another
server.

## How it is built

- **Specification first.** `spec/` is a Loft specification of the
  three products: requirements, acceptance criteria, components and decisions. It is the
  contract, published from `develop`. Behaviour changes in the specification first, then in the
  code.
- **Traced tests.** `loft.trace.yaml` links every acceptance criterion to the tests that verify
  it.
- **One gate.** `cargo xtask check` runs:
  - `fmt` and clippy (pedantic, with no `unwrap`, `panic` or `unsafe`);
  - a limit of 500 lines per Rust file;
  - the Rust tests and `cargo deny`;
  - the specification checks;
  - the explorer's typecheck, tests and build.
- **End-to-end.** `cargo xtask integration` starts the Worker on fixture releases, then runs its
  end-to-end tests and the explorer's Playwright tests, which include accessibility checks.
- **No network in tests.** Tests read recorded provider answers.

`AGENTS.md` has the full list of commands and conventions.

## Layout

| Path | What it holds |
|---|---|
| `spec/` | The Loft specification |
| `sources/`, `structures/`, `stars/`, `names/` | Source definitions and researched data, in YAML |
| `rules/` | Matching, ranking, designation, tile and science-check rules |
| `crates/` | `univ-model`, `univ-ingest`, `univ-api` and `univ-server` |
| `clients/` | `explorer` and `tile-decoder` |
| `xtask/` | The gate, integration runs, science checks, benchmarks and `explore` |
| `.yard/` | Each branch's record of how its work was planned, decided and done |
