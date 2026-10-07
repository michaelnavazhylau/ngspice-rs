import tailwind from "bun-plugin-tailwind";
import { rm } from "node:fs/promises";
import { buildWorker } from "./build-worker.ts";

await rm("dist", { recursive: true, force: true });
const res = await Bun.build({
  entrypoints: ["index.html"],
  outdir: "dist",
  minify: true,
  target: "browser",
  publicPath: "./",
  plugins: [tailwind],
});
if (!res.success) {
  for (const l of res.logs) console.error(l);
  process.exit(1);
}
await buildWorker("dist");
for (const o of res.outputs) console.log(o.path.replace(process.cwd() + "/", ""), (o.size / 1024).toFixed(1) + " KB");
console.log("dist/worker.js + wasm written");
