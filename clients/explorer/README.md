# Univ Explorer

A web app that draws a release of Univ Server in 3D around Earth: the stars, the nearby galaxies
and the cosmic web of every catalogue the release holds, and its large-scale structures. It is
driven by the mouse and reads every object and structure from Univ Server. Its specification is
the `univ-explorer` product in `spec/`.

## Run it

From the workspace root (`univ-server/`), with a release published by `univ-ingest publish`:

```sh
cargo xtask explore
```

Then open http://127.0.0.1:5317/. The first build of the Worker takes a few minutes; Ctrl-C
stops both servers. The command starts Univ Server on the real local state, issues the explorer a
key of the extended tier (anonymous reads are limited to 300 a minute), and runs the explorer's
dev server, which passes its reads on to the Worker with that key. The key stays on the server.

To run the explorer against a Univ Server you started yourself:

```sh
cd clients/explorer
UNIV_SERVER=http://127.0.0.1:8790 UNIV_KEY=<an API key> pnpm dev
```

## Use it

| To | Do |
|---|---|
| Turn the view about the focus | Drag with the left button |
| Move the focus across the view | Drag with the right button, or with Shift held |
| Come closer or go farther | Scroll: each step is the same factor, from 0.1 pc to 20 Gpc |
| Read an object | Hover a point for its label; click it for its record |
| Fly to an object | Double-click it (it moves at once if your system asks for reduced motion) |
| Find an object | Type an id, `source:identifier` (such as `hipparcos-2007:32349`) or a name, and Find |
| See a structure | Click its name in the view, or choose it from the list of structures |
| Come home | Return to Earth |

The view's axes are galactic, equatorial (the ICRS) or supergalactic; the ring is the chosen
frame's equator around Earth. Positions are ICRS Cartesian about the Solar System barycentre,
which the view marks as Earth (within about 1 AU of it). The key counts the points of each kind of
value of their distances: measured, estimated, inferred and reconstructed, each in its colour.
The credits name every source of the drawn points, with its citation and required credit.

The chosen object is kept in the address (`?object=<id>`), so a reload or a shared link shows it
again.

## How it is built

- **Tiles.** Each scale domain's octree of tiles is fetched root first, then by size on screen,
  within 3,000,000 points. Each tile is checked against its manifest's SHA-256 and decoded by the
  reference decoder in `../tile-decoder`; a tile that fails is not drawn and is counted.
- **Rendering.** WebGL2, drawn by `src/scene/renderer.ts`: points blended additively, lines for
  rings and outlines. Each tile's points are float32 offsets from its centre, placed relative to
  the eye in double precision, so parsecs stay sharp a hundred megaparsecs out.
- **Picking.** The pointer's ray is tested against each tile's bounding sphere; only the tiles
  along it are read.
- **Structures.** Spheres, ellipsoids and boxes are drawn as outlines, paths as lines, members and
  points as markers; sky regions, grids and structures with no placed shape as their bounding
  box, marked approximate.

## Check it

```sh
pnpm check   # typecheck, Vitest with coverage, fallow, build
pnpm e2e     # Playwright and axe, against the Univ Server in UNIV_SERVER
```

`cargo xtask check` runs `pnpm check`; `cargo xtask integration` runs `pnpm e2e` against the
fixture release. `cargo xtask bench first-points` measures the first points drawn on release 7
(the 95th percentile of 10 loads, against 3 s).
