/**
 * `bun run build:wasm`: compile crates/spice-wasm for wasm32 and generate the
 * JS bindings into src/wasm/pkg.
 *
 * wasm-bindgen's CLI must be the exact version of the `wasm-bindgen` crate in
 * Cargo.lock (the two share an internal schema), so the script checks that
 * first and prints the install command on a mismatch instead of failing later
 * with an opaque schema error.
 */
import { $ } from "bun";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..");
const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
const locked = /\[\[package\]\]\nname = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)?.[1];
if (!locked) throw new Error("wasm-bindgen is not in Cargo.lock; is crates/spice-wasm a workspace member?");

const cli = (await $`wasm-bindgen --version`.nothrow().quiet()).stdout.toString().trim();
const installed = /wasm-bindgen (\S+)/.exec(cli)?.[1];
if (installed !== locked) {
  console.error(
    `wasm-bindgen CLI ${installed ?? "is not installed"}, but Cargo.lock pins ${locked}.\n` +
      `Install the matching CLI:\n  cargo install wasm-bindgen-cli --version ${locked} --locked`,
  );
  process.exit(1);
}
if (!(await $`rustup target list --installed`.quiet().text()).includes("wasm32-unknown-unknown")) {
  console.error("The wasm32 target is missing:\n  rustup target add wasm32-unknown-unknown");
  process.exit(1);
}

await $`cargo build --release --target wasm32-unknown-unknown -p spice-wasm --manifest-path ${join(root, "Cargo.toml")}`;
await $`wasm-bindgen --target web --out-dir ${join(import.meta.dir, "..", "src/wasm/pkg")} ${join(root, "target/wasm32-unknown-unknown/release/spice_wasm.wasm")}`;
console.log(`spice-wasm built with wasm-bindgen ${locked} -> src/wasm/pkg`);
