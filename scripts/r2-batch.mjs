// Every R2 operation of one `univ-ingest publish` or `prune` run, through one process.
//
//   node scripts/r2-batch.mjs --persist-to <dir> --bucket <name>
//
// It opens the local R2 bucket that `wrangler r2 object … --local --persist-to <dir>` opens,
// the way Wrangler does (its `usingLocalBucket`, wrangler 4.156.2): the Miniflare that Wrangler
// itself loads, given `<dir>/v3` as its resource persistence path and the bucket bound as
// `BUCKET`. An object it writes is one `wrangler r2 object get --local` reads, and the reverse.
//
// It reads one operation per line of standard input, as JSON, and writes one result line per
// object to standard output, as JSON, in the order of the operations:
//
//   {"op":"put","key":K,"file":F,"contentType":T}
//     writes the file F at K, as `wrangler r2 object put` does, then reads K back from R2:
//     {"key":K,"ok":true,"found":true,"bytes":N,"sha256":H}   what R2 gave back
//     {"key":K,"ok":true,"found":false}                       R2 held nothing at K
//   {"op":"get","key":K,"file":F}
//     writes the object at K to the file F:
//     {"key":K,"ok":true,"found":true,"bytes":N} or {"key":K,"ok":true,"found":false}
//   {"op":"delete","keys":[K, …]}
//     deletes the keys, at most 1,000 to a call of R2's delete; one line per key:
//     {"key":K,"ok":true}
//
// A failure is {"key":K,"ok":false,"error":E}, naming the object and the cause; a delete that
// fails gives it for each key of its call, and "not attempted" for each key after them. An
// operation that cannot be read is {"ok":false,"error":E}. The process ends when standard input
// does. Anything else Miniflare or workerd prints goes to standard error.

import { createHash } from "node:crypto";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import readline from "node:readline";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

/** The most keys R2's delete takes in one call. */
const DELETE_CHUNK = 1000;

// Results alone go to standard output; everything else printed goes to standard error.
const results = process.stdout.write.bind(process.stdout);
process.stdout.write = process.stderr.write.bind(process.stderr);
const reply = (line) =>
  new Promise((resolve, reject) =>
    results(`${JSON.stringify(line)}\n`, (e) => (e ? reject(e) : resolve())),
  );

function argument(name) {
  const at = process.argv.indexOf(name);
  if (at < 0 || at + 1 >= process.argv.length) {
    throw new Error(`usage: r2-batch.mjs --persist-to <dir> --bucket <name>: no ${name}`);
  }
  return process.argv[at + 1];
}

/** The message of an error, with its causes. */
function said(e) {
  let message = e?.message ?? String(e);
  for (let c = e?.cause; c !== undefined && c !== null; c = c.cause) {
    message += `: ${c?.message ?? String(c)}`;
  }
  return message;
}

// The Miniflare Wrangler loads, resolved from Wrangler's own package.
const here = createRequire(import.meta.url);
const fromWrangler = createRequire(here.resolve("wrangler/package.json"));
const { Miniflare, convertV4MiniflareOptions } = fromWrangler("miniflare");

// Wrangler's `getLocalPersistencePath` and `getDefaultPersistRoot`.
const persistTo = path.resolve(process.cwd(), argument("--persist-to"));
const bucketName = argument("--bucket");

// Wrangler's put Worker, as `usingLocalBucket` gives it.
const script = `
    function reduceError(e) {
      return {
        name: e?.name,
        message: e?.message ?? String(e),
        stack: e?.stack,
        cause: e?.cause === undefined ? undefined : reduceError(e.cause),
      };
    }
    export default {
      async fetch(request, env, ctx) {
        try {
          if (request.method !== "PUT") return new Response(null, { status: 405 });
          const url = new URL(request.url);
          const key = decodeURIComponent(url.pathname.substring(1));
          const optsHeader = request.headers.get("Wrangler-R2-Put-Options");
          const opts = JSON.parse(optsHeader);
          await env.BUCKET.put(key, request.body, opts);
          return new Response(null, { status: 204 });
        } catch (e) {
          const error = reduceError(e);
          return Response.json(error, {
            status: 500,
            headers: { "MF-Experimental-Error-Stack": "true" },
          });
        }
      }
    }`;

const mf = new Miniflare(
  convertV4MiniflareOptions({
    modules: true,
    script,
    resourcePersistencePath: path.join(persistTo, "v3"),
    r2Buckets: { BUCKET: bucketName },
  }),
);

/** Writes the file at `key` as `wrangler r2 object put --local` does. */
async function put(key, file, contentType) {
  const size = fs.statSync(file).size;
  const response = await mf.dispatchFetch(`http://localhost/${encodeURIComponent(key)}`, {
    method: "PUT",
    body: Readable.toWeb(fs.createReadStream(file)),
    duplex: "half",
    headers: {
      "Content-Length": String(size),
      "Wrangler-R2-Put-Options": JSON.stringify({ httpMetadata: { contentType } }),
    },
  });
  if (response.status !== 204) {
    const text = await response.text();
    let message = text;
    try {
      message = JSON.parse(text).message ?? text;
    } catch {}
    throw new Error(`R2 refused the write (HTTP ${response.status}): ${message}`);
  }
}

/** The size and SHA-256 of the object at `key` as R2 gives it back, or null. */
async function readBack(bucket, key) {
  const object = await bucket.get(key);
  if (object === null) return null;
  const hash = createHash("sha256");
  let bytes = 0;
  for await (const chunk of object.body) {
    hash.update(chunk);
    bytes += chunk.length;
  }
  return { bytes, sha256: hash.digest("hex") };
}

async function run(bucket, op) {
  switch (op.op) {
    case "put": {
      try {
        await put(op.key, op.file, op.contentType);
        const back = await readBack(bucket, op.key);
        await reply(
          back === null
            ? { key: op.key, ok: true, found: false }
            : { key: op.key, ok: true, found: true, ...back },
        );
      } catch (e) {
        await reply({ key: op.key, ok: false, error: said(e) });
      }
      return;
    }
    case "get": {
      try {
        const object = await bucket.get(op.key);
        if (object === null) {
          await reply({ key: op.key, ok: true, found: false });
          return;
        }
        fs.mkdirSync(path.dirname(op.file), { recursive: true });
        await pipeline(object.body, fs.createWriteStream(op.file));
        await reply({ key: op.key, ok: true, found: true, bytes: fs.statSync(op.file).size });
      } catch (e) {
        await reply({ key: op.key, ok: false, error: said(e) });
      }
      return;
    }
    case "delete": {
      const keys = op.keys ?? [];
      let failed = null;
      for (let at = 0; at < keys.length; at += DELETE_CHUNK) {
        const chunk = keys.slice(at, at + DELETE_CHUNK);
        if (failed !== null) {
          for (const key of chunk) {
            await reply({ key, ok: false, error: "not attempted: an earlier delete failed" });
          }
          continue;
        }
        try {
          await bucket.delete(chunk);
          for (const key of chunk) await reply({ key, ok: true });
        } catch (e) {
          failed = said(e);
          for (const key of chunk) await reply({ key, ok: false, error: failed });
        }
      }
      return;
    }
    default:
      await reply({ ok: false, error: `no operation ${JSON.stringify(op.op)}` });
  }
}

try {
  const bucket = await mf.getR2Bucket("BUCKET");
  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (line.trim() === "") continue;
    let op;
    try {
      op = JSON.parse(line);
    } catch (e) {
      await reply({ ok: false, error: `not an operation: ${said(e)}` });
      continue;
    }
    await run(bucket, op);
  }
} finally {
  await mf.dispose();
}
