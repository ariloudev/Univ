# Univ Explorer

A web app that draws a release of Univ Server in 3D around Earth: the stars, the nearby galaxies
and the cosmic web of every catalogue the release holds, and its large-scale structures. It is
driven by the mouse and reads every object and structure from Univ Server. Its specification is
the `univ-explorer` product in `spec/`.

## Run it

From the workspace root (`univ-server/`), with a release published by `univ-ingest publish` and
the owner secret in `crates/univ-server/.dev.vars` (`OWNER_SECRET=...`; git ignores the file):

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
| See a structure | Click its name in the view (hovering it says so), or choose it from the list of structures |
| Come home | Return to Earth |

The view's axes are galactic, equatorial (the ICRS) or supergalactic; the ring is the chosen
frame's equator around Earth. Positions are ICRS Cartesian about the Solar System barycentre,
which the view marks as Earth (within about 1 AU of it). The key counts the points of each kind of
value of their distances: measured, estimated, inferred and reconstructed, all drawn in one colour.
The credits name every source of the drawn points, with its citation and required credit.

The chosen object is kept in the address (`?object=<id>`), so a reload or a shared link shows it
again.

## How it is built

- **Tiles.** Each scale domain is an octree whose inner tiles hold a sample of the points under
  them. The view wants every root, then, the largest on screen first, the children in view of
  each wanted tile whose level's typical tile (the mean points of its split tiles, its edge)
  would have its points more than 3 px apart on screen, all of a tile's children
  together, within 6,000,000 points. Tiles are fetched as the view wants them; held tiles it no
  longer wants are dropped only when the budget needs room, those out of view first, then the
  smallest on screen, never a root nor a tile whose children are held. A dropped tile stays
  drawn until no tile is on its way, within a quarter of the budget beyond it, so a moving view
  shows no hole while new tiles arrive. A held tile's children are kept down to 2 px
  and 1° beyond the view, and a family that holds a tile counts half again its size, so that a
  view moving a little does not drop and fetch the same tiles over and over. Once read, a tile is
  measured by the distance of its points, not of its cube, which can be far larger. Points are
  drawn one by one, not tile by tile: each level of a domain holds a slice of it (from what the
  levels above keep to what it adds, as the manifest's fractions give), a tile's points, shuffled
  by its id on decoding, divide its slice into ranks, and the point shader draws a point while its
  rank lies below the share of the domain the view needs at the point's own distance. That share
  rises level by level as a typical tile of the level above has its points from 3 to 6 px apart,
  so density thins smoothly with distance, regions at one distance keep their relative density,
  and no tile's cube shows as an edge. A tile also fades in and out over 0.3 s. Only the points
  drawn can be picked. Thinned, a domain's points stay fully opaque as the eye withdraws, fading
  only beyond the distance where its root alone is drawn (with level of detail off, beyond 1 Gpc). The **Level of
  detail** switch in the top bar turns all this off (`?lod=off` in the address): every tile is
  fetched, the coarser levels first, whatever the view and the budget, and none is dropped;
  turned on again, the tiles the view does not want are dropped until the budget holds. Each tile is
  checked against its manifest's SHA-256 and decoded by the reference decoder in
  `../tile-decoder`; a tile that fails is not drawn and is counted.
- **Culling.** Only the tiles whose points' sphere reaches into the view are drawn.
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
fixture release, installing Playwright's Chromium first where it is missing. `cargo xtask bench
first-points` measures the first points drawn on the latest published release: one load to warm
up, then the 95th percentile of 10, against 3 s.
