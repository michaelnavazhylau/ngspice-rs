import type { Analysis, Doc, VFile } from "./circuit/model.ts";
import { defaultAnalysis } from "./circuit/model.ts";
import { PARAMS_FILE } from "./circuit/examples.ts";

const PROJECT = "spice-web:v1:project";
const FILES = "spice-web:v1:files";
const UI = "spice-web:v1:ui";

export interface Project {
  version: 1;
  doc: Doc;
  analysis: Analysis;
  deckOverride?: string | null;
}

function read<T>(key: string): T | null {
  try {
    const s = localStorage.getItem(key);
    return s ? (JSON.parse(s) as T) : null;
  } catch {
    return null;
  }
}
function write(key: string, v: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* storage unavailable or full: ignore */
  }
}

/** Validate untrusted JSON (localStorage or imported file) into a Project. */
export function parseProject(raw: unknown): Project | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const doc = r.doc as Doc | undefined;
  if (!doc || !Array.isArray(doc.components) || !Array.isArray(doc.wires)) return null;
  const okC = doc.components.every(
    (c) => c && typeof c.id === "string" && typeof c.kind === "string" && Number.isFinite(c.x) && Number.isFinite(c.y),
  );
  const okW = doc.wires.every((w) => w && typeof w.id === "string" && w.a && w.b);
  if (!okC || !okW) return null;
  const analysis = { ...defaultAnalysis(), ...((r.analysis as Partial<Analysis>) ?? {}) };
  return { version: 1, doc, analysis, deckOverride: typeof r.deckOverride === "string" ? r.deckOverride : null };
}

export const loadProject = (): Project | null => parseProject(read(PROJECT));
export const saveProject = (p: Project): void => write(PROJECT, p);

export function loadFiles(): VFile[] {
  const f = read<VFile[]>(FILES);
  if (Array.isArray(f) && f.every((x) => typeof x?.name === "string" && typeof x?.content === "string")) return f;
  return [PARAMS_FILE];
}
export const saveFiles = (f: VFile[]): void => write(FILES, f);

export interface UiPrefs {
  resultsHeight: number;
}
export const loadUi = (): UiPrefs => ({ resultsHeight: 280, ...(read<UiPrefs>(UI) ?? {}) });
export const saveUi = (u: UiPrefs): void => write(UI, u);
