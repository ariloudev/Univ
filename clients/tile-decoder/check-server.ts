// Fetches every tile of a release from a running Univ Server and checks it with the decoder:
// each tile's SHA-256 is the one its manifest gives, and it decodes, which checks its node, its
// count of points and every value it holds. One line per scale domain; the exit status is 1 on
// any failure.
//
//   node --experimental-strip-types check-server.ts http://127.0.0.1:8787 [release]
//
// Without a release it reads the latest, and pins every later request to the release the
// listing names. An anonymous client may make 300 requests a minute: give an API key in
// UNIV_KEY to go faster. A 429 is waited out as its Retry-After says.
//
// Node's own fetch and crypto alone: no dependencies.

import { createHash } from 'node:crypto';

import { decodeTile, type TileManifest } from './decode.ts';

interface ScaleDomainList {
  release: number;
  domains: { name: string }[];
}

const [base, asked] = process.argv.slice(2);
if (base === undefined) {
  console.error('usage: check-server.ts <base url> [release]');
  process.exit(2);
}
const key = process.env.UNIV_KEY;
const headers: Record<string, string> = key === undefined ? {} : { authorization: `Bearer ${key}` };

async function get(path: string, release: string | undefined): Promise<Uint8Array> {
  const url = new URL(path, base);
  if (release !== undefined) {
    url.searchParams.set('release', release);
  }
  for (;;) {
    const response = await fetch(url, { headers });
    if (response.status === 429) {
      const seconds = Number(response.headers.get('retry-after') ?? '60');
      await new Promise((resolve) => setTimeout(resolve, seconds * 1000));
      continue;
    }
    if (response.status !== 200) {
      throw new Error(`${url.pathname}: ${response.status} ${await response.text()}`);
    }
    return new Uint8Array(await response.arrayBuffer());
  }
}

async function getJson<T>(path: string, release: string | undefined): Promise<T> {
  return JSON.parse(new TextDecoder().decode(await get(path, release))) as T;
}

const listing = await getJson<ScaleDomainList>('/v1/scale-domains', asked);
const release = String(listing.release);
let failures = 0;
for (const domain of listing.domains) {
  const at = `/v1/scale-domains/${encodeURIComponent(domain.name)}`;
  const manifest = await getJson<TileManifest>(`${at}/manifest`, release);
  const problems: string[] = [];
  let points = 0;
  let bytes = 0;
  for (const node of manifest.nodes) {
    const tile = await get(`${at}/tiles/${encodeURIComponent(node.address)}`, release);
    bytes += tile.length;
    const sha256 = createHash('sha256').update(tile).digest('hex');
    if (sha256 !== node.sha256) {
      problems.push(`${node.address}: SHA-256 ${sha256}, the manifest gives ${node.sha256}`);
      continue;
    }
    try {
      points += decodeTile(tile, node, manifest).length;
    } catch (error) {
      problems.push(`${node.address}: ${(error as Error).message}`);
    }
  }
  failures += problems.length;
  const verdict = problems.length === 0 ? 'ok' : `${problems.length} FAILED`;
  console.log(
    `release ${release} ${domain.name}: ${manifest.nodes.length} tiles, ${bytes} bytes, ` +
      `${points} points decoded: ${verdict}`,
  );
  for (const problem of problems.slice(0, 10)) {
    console.log(`  ${problem}`);
  }
}
process.exit(failures === 0 ? 0 : 1);
