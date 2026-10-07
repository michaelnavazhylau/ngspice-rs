import { buildWorker } from "./build-worker.ts";

const dist = process.argv.includes("--dist");
const port = Number(process.env.PORT ?? 3000);

// The worker and its .wasm are bundled separately and served from memory (dev)
// or from dist/ (--dist preview).
const index = dist ? null : (await import("./index.html")).default;
const assets = new Map<string, Blob>();
if (!dist) {
  const res = await buildWorker();
  for (const o of res.outputs) assets.set("/" + o.path.replace(/^\.\//, ""), o);
}

const server = Bun.serve({
  port,
  routes: index ? { "/": index } : undefined,
  async fetch(req) {
    const path = new URL(req.url).pathname;
    if (dist) {
      const file = Bun.file("dist" + (path === "/" ? "/index.html" : path));
      return (await file.exists()) ? new Response(file) : new Response("not found", { status: 404 });
    }
    const a = assets.get(path);
    if (a) return new Response(a, { headers: { "content-type": a.type } });
    return new Response("not found", { status: 404 });
  },
  development: !dist && { hmr: true, console: true },
});
console.log(`spice-web ${dist ? "(dist) " : ""}at ${server.url}`);
