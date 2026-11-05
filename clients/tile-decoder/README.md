# Tile decoder

The reference TypeScript decoder of Univ Server's point tiles (decode-the-tile-format). It turns
the bytes of one octree node's tile into the points it encodes, using that scale domain's tile
manifest. It has no dependencies, and Node runs it straight from its `.ts` files by stripping
the types, so it uses only TypeScript syntax that can be erased (no enums, namespaces or
parameter properties).

## Running the tests

You need Node 22.6 or later. Node 23.6 and later strip types by default; on 22.6 to 23.5, add
`--experimental-strip-types`.

```sh
cd clients/tile-decoder
node --test '*.test.ts'
```

`cargo xtask check` runs the same tests in its `tile decoder` gate. The gate is skipped when
`node` is not on the path, and fails when Node is older than 22.6.

The fixtures in `fixtures/` are JSON:

- `<domain>.manifest.json` is a tile manifest exactly as Univ Ingest writes it.
- `<domain>.tiles.json` holds that domain's tiles in hexadecimal, as the Rust encoder wrote them.
  Beside each point it gives the position before quantization, the object id, the source, the
  kind of value, and the colour index before rounding with its bands, or `null`.
- `v1/` keeps the SIMULATED domain as encoding version 1 wrote it, before version 2: it is never
  rewritten, so the decoder keeps reading the tiles of releases built before version 2.

The fixtures cover the three scale domains of Univ Ingest's fixture release, which is real
data, and one SIMULATED domain (`simulated.*`) with several levels, two sources and every kind
of value. univ-ingest's test `tile_decoder_fixtures` checks that the fixtures still match what
ingest builds. To rewrite them:

```sh
UPDATE_FIXTURES=1 cargo test -p univ-ingest --test tile_decoder_fixtures
```

## API

```ts
import { decodeTile, VERSIONS } from './decode.ts';

const points = decodeTile(bytes, node, manifest);
// [{ x, y, z, id, source, kind, colour? }, ...] in the order the tile stores them
```

- `bytes: Uint8Array` is the tile of `node`, such as `releases/{n}/tiles/{domain}/{address}.bin`.
- `node: ManifestNode` is the entry for that tile in `manifest.nodes`.
- `manifest: TileManifest` is the scale domain's `manifest.json`, parsed.
- Each point gives:
  - `x`, `y`, `z`: the position in the manifest's frame (ICRS equatorial Cartesian, origin at
    the Solar System barycentre) and unit (`pc` or `Mpc`). It is the centre of the position's
    quantization step, so it lies within half of the level's `step` of the position the tile
    was built from.
  - `id`: the object id, a number below 2^53.
  - `source`: the entry in `manifest.sources` for the source of the point's preferred distance,
    with its title, citation and required credit.
  - `kind`: the kind of value of that distance, one of `measured`, `estimated`, `inferred` or
    `reconstructed`.
  - `colour`, in a version 2 tile where the point's object has one: `{ index, bands }`, its
    colour index in magnitudes to the hundredth and the bands it lies between, `BP-RP` or
    `B-V`. Absent in a version 1 tile and for a point with none.
- `VERSIONS` lists the encoding versions the decoder knows: `[1, 2]`.

`decodeTile` throws an `Error` in these cases:

- the magic is not `UVTL`;
- the encoding version is one it does not know (the message names the version; the version is
  read before the size, so this holds at any size);
- a reserved field is not zero;
- the size is not `48 + 17 n` for the tile's count `n`, or not the node's `points` and `bytes`;
- the header's corner and edge are not the node's bounds and its level's step;
- an object id is outside 1 to 2^53 − 1;
- a source or kind of value is not listed in the manifest.

It does not check the tile's SHA-256 against `node.sha256`; a client that wants to can use
`crypto.subtle.digest('SHA-256', bytes)`.

## Encoding version 1

Every number is little-endian, with no alignment or padding. A tile of `n` points is
`48 + 17 n` bytes:

| Offset | Type | Field |
|---|---|---|
| 0 | 4 bytes | Magic `UVTL` |
| 4 | u16 | Encoding version, 1 |
| 6 | u16 | Reserved, 0 |
| 8 | u32 | Point count `n` |
| 12 | u32 | Reserved, 0 |
| 16 | 3 × f64 | The node's minimum corner x, y, z |
| 40 | f64 | The node's edge |
| 48 | 3n × u16 | Each point's quantized x, y, z |
| 48 + 6n | n × u64 | Each point's object id |
| 48 + 14n | n × u16 | Each point's index in `manifest.sources` |
| 48 + 16n | n × u8 | Each point's index in `manifest.encoding.kinds` |

The step is `edge / 65536`. A coordinate stored as `q` decodes to `min + (q + 0.5) * step`.

The Rust encoder and reference decoder are in `crates/univ-model/src/tile/`.
`univ_model::tile::format::layout` gives the same layout as data, for Univ Server's tile format
answer.

## Read tiles from the release of their manifest

A tile is decoded against the manifest of its own release. Fetch the manifest pinned to a
release (the scale-domain listing gives that URL), and fetch each tile with the same
`?release=n`. A tile fetched unpinned is redirected to the latest release's tile, which may be
a later release than a manifest read a moment before.

## Check every tile a server serves

`check-server.ts` fetches each scale domain's manifest and every tile of a release from a
running Univ Server, such as one `cargo xtask serve` starts. It checks each tile's SHA-256
against the manifest and decodes it; decoding checks the tile's node, its count of points and
every value it holds:

```sh
UNIV_KEY=<an API key> node --experimental-strip-types check-server.ts http://127.0.0.1:8787 7
```

Without a release it reads the latest, and pins every later request to the release the listing
names. An anonymous client may make 300 requests a minute, fewer than a release's tiles: a key
in `UNIV_KEY` goes faster, and a 429 is waited out as its `Retry-After` says. It prints one line
per scale domain and exits with status 1 on any failure.
