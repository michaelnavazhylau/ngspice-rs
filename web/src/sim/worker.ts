/// <reference lib="webworker" />
import init, { simulate, version } from "../wasm/pkg/spice_wasm.js";
import wasmUrl from "../wasm/pkg/spice_wasm_bg.wasm";
import type { WorkerIn, WorkerOut } from "./types.ts";

const post = (m: WorkerOut) => postMessage(m);
let ready: Promise<void> | null = null;

function boot(): Promise<void> {
  ready ??= init({ module_or_path: wasmUrl as unknown as string }).then(() => {
    post({ type: "ready", version: version() });
  });
  return ready;
}

self.onmessage = async (ev: MessageEvent<WorkerIn>) => {
  const m = ev.data;
  try {
    await boot();
  } catch (e) {
    post({ type: "fatal", message: e instanceof Error ? e.message : String(e) });
    return;
  }
  if (m.type !== "run") return;
  const t0 = performance.now();
  try {
    const json = simulate(m.deck, m.names, m.contents);
    post({ type: "result", id: m.id, json, ms: performance.now() - t0 });
  } catch (e) {
    post({ type: "error", id: m.id, message: e instanceof Error ? e.message : String(e) });
  }
};
