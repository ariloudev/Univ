// Fetches every tile of a release from a running Univ Server and checks it with the decoder:
// each tile's SHA-256 is the one its manifest gives, it decodes, it holds the node's count of
// points, and each point lies within its node's cube. One line per scale domain; the exit
// status is 1 on any failure.
//
//   node --experimental-strip-types check-server.ts http://127.0.0.1:8787 [release]
//
// An anonymous client may make 300 requests a minute, fewer than a release's tiles: give an API
// key, of the extended tier for a large release, in UNIV_KEY.
//
// Node's own fetch and crypto alone: no dependencies.

import { createHash } from 'node:crypto';

import { decodeTile, type TileManifest } from './decode.ts';

interface ScaleDomainList {
  release: number;
  domains: { name: string; manifest: string; nodes: number; points: number }[];
}

const [base, release] = process.argv.slice(2);
if (base === undefined) {
  console.error('usage: check-server.ts <base url> [release]');
  process.exit(2);
}
const pin = release === undefined ? '' : `?release=${release}`;

async function get(path: string): Promise<Uint8Array> {
  const key = process.env.UNIV_KEY;
  const headers: Record<string, string> = key === undefined ? {} : { authorization: `Bearer ${key}` };
  const response = await fetch(`${base}${path}${pin}`, { headers });
  if (response.status !== 200) {
    throw new Error(`${path}: ${response.status} ${await response.text()}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

const listing: ScaleDomainList = JSON.parse(new TextDecoder().decode(await get('/v1/scale-domains')));
let failures = 0;
for (const domain of listing.domains) {
  const manifest: TileManifest = JSON.parse(
    new TextDecoder().decode(await get(`/v1/scale-domains/${domain.name}/manifest`)),
  );
  const problems: string[] = [];
  let points = 0;
  let bytes = 0;
  for (const node of manifest.nodes) {
    const tile = await get(`/v1/scale-domains/${domain.name}/tiles/${node.address}`);
    bytes += tile.length;
    const sha256 = createHash('sha256').update(tile).digest('hex');
    if (sha256 !== node.sha256) {
      problems.push(`${node.address}: SHA-256 ${sha256}, the manifest gives ${node.sha256}`);
      continue;
    }
    try {
      const decoded = decodeTile(tile, node, manifest);
      if (decoded.length !== node.points) {
        problems.push(`${node.address}: ${decoded.length} points, the manifest gives ${node.points}`);
      }
      const outside = decoded.filter((p) =>
        [p.x, p.y, p.z].some((c, i) => c < node.bounds.min[i] || c > node.bounds.max[i]),
      );
      if (outside.length > 0) {
        problems.push(`${node.address}: ${outside.length} points outside the node's cube`);
      }
      points += decoded.length;
    } catch (error) {
      problems.push(`${node.address}: ${(error as Error).message}`);
    }
  }
  failures += problems.length;
  const verdict = problems.length === 0 ? 'ok' : `${problems.length} FAILED`;
  console.log(
    `release ${listing.release} ${domain.name}: ${manifest.nodes.length} tiles, ${bytes} bytes, ` +
      `${points} points decoded: ${verdict}`,
  );
  for (const problem of problems.slice(0, 10)) {
    console.log(`  ${problem}`);
  }
}
process.exit(failures === 0 ? 0 : 1);
