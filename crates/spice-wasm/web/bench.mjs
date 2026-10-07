// WASM counterpart of examples/bench.rs.
// usage: node bench.mjs PKG_DIR WASM_FILE REPS DECK...
// PKG_DIR holds `wasm-bindgen --target web` output; WASM_FILE may be a
// wasm-opt'd replacement for its `_bg.wasm`.
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, wasmFile, repsText, ...decks] = process.argv.slice(2);
const t0 = performance.now();
const bindings = await import(pathToFileURL(`${pkg}/spice_wasm.js`).href);
bindings.initSync({ module: readFileSync(wasmFile) });
console.error(`instantiate ${(performance.now() - t0).toFixed(1)} ms`);
const reps = Number(repsText);

const fnv = (text) => {
  let h = 0xcbf29ce484222325n;
  for (const b of Buffer.from(text)) h = BigInt.asUintN(64, (h ^ BigInt(b)) * 0x100000001b3n);
  return h.toString(16).padStart(16, "0");
};

for (const path of decks) {
  const text = readFileSync(path, "utf8");
  const name = basename(path, ".cir");
  const times = [];
  let raw, err;
  for (let i = 0; i < reps; i++) {
    const start = performance.now();
    try { raw = bindings.simulate(text); } catch (e) { err = String(e.message ?? e); }
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  if (err) console.log(`${name}\tERR\t${err.split("\n")[0]}`);
  else console.log(`${name}\t${times[reps >> 1].toFixed(3)}\t${raw.length}\t${fnv(raw)}`);
}
