/** Bundles the simulator worker (and the .wasm as a file asset) – shared by dev server and build. */
export async function buildWorker(outdir?: string) {
  const res = await Bun.build({
    entrypoints: ["src/sim/worker.ts"],
    target: "browser",
    format: "esm",
    minify: outdir !== undefined,
    publicPath: "./",
    naming: { entry: "worker.js", asset: "[name]-[hash].[ext]" },
    ...(outdir ? { outdir } : {}),
  });
  if (!res.success) {
    for (const l of res.logs) console.error(l);
    throw new Error("worker build failed");
  }
  return res;
}
