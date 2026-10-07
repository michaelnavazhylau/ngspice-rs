import type { SimResult, WorkerOut } from "./types.ts";

export interface RunOutcome {
  result: SimResult;
  /** time spent inside the engine (worker), ms */
  engineMs: number;
  /** wall-clock including messaging, ms */
  wallMs: number;
}

export class SimCancelled extends Error {
  constructor() {
    super("Simulation cancelled");
  }
}

/** Typed wrapper around the simulator Web Worker. Cancel = terminate + recreate. */
export class SimEngine {
  version: string | null = null;
  fatal: string | null = null;
  private worker!: Worker;
  private nextId = 1;
  private pending = new Map<
    number,
    { resolve: (o: RunOutcome) => void; reject: (e: Error) => void; t0: number }
  >();
  private listeners = new Set<() => void>();

  constructor() {
    this.spawn();
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
  private emit() {
    for (const l of this.listeners) l();
  }

  private spawn() {
    this.worker = new Worker(new URL("worker.js", document.baseURI), { type: "module" });
    this.worker.onmessage = (ev: MessageEvent<WorkerOut>) => this.onMessage(ev.data);
    this.worker.onerror = (ev) => {
      this.fatal = ev.message || "worker failed";
      for (const [, p] of this.pending) p.reject(new Error(this.fatal));
      this.pending.clear();
      this.emit();
    };
    this.worker.postMessage({ type: "init" });
  }

  private onMessage(m: WorkerOut) {
    switch (m.type) {
      case "ready":
        this.version = m.version;
        this.fatal = null;
        this.emit();
        break;
      case "fatal":
        this.fatal = m.message;
        for (const [, p] of this.pending) p.reject(new Error(m.message));
        this.pending.clear();
        this.emit();
        break;
      case "result": {
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id);
        try {
          p.resolve({
            result: JSON.parse(m.json) as SimResult,
            engineMs: m.ms,
            wallMs: performance.now() - p.t0,
          });
        } catch (e) {
          p.reject(new Error(`Bad result from engine: ${String(e)}`));
        }
        break;
      }
      case "error": {
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id);
        p.reject(new Error(m.message));
        break;
      }
    }
  }

  run(deck: string, files: { name: string; content: string }[]): Promise<RunOutcome> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, t0: performance.now() });
      this.worker.postMessage({
        type: "run",
        id,
        deck,
        names: files.map((f) => f.name),
        contents: files.map((f) => f.content),
      });
    });
  }

  cancel(): void {
    this.worker.terminate();
    for (const [, p] of this.pending) p.reject(new SimCancelled());
    this.pending.clear();
    this.spawn();
  }
}

/** The engine reports locations against a virtual "/deck.cir"; turn that into "line N". */
export function prettyError(message: string): string {
  return message.replace(/\/?deck\.cir:(\d+)(?::\d+)?:?/g, "line $1:");
}
