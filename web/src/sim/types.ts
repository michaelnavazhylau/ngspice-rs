export interface Variable {
  name: string;
  unit: string;
  re: (number | null)[];
  im?: (number | null)[];
}
export interface Plot {
  name: string;
  type: string;
  complex: boolean;
  variables: Variable[];
}
/** A `.measure` result. */
export interface Measurement {
  name: string;
  value: number | null;
  /** operand or axis unit as the engine names it: voltage, current, time, … */
  unit: string;
  /** axis position of a FIND/MIN/MAX result */
  at: number | null;
}
export interface Harmonic {
  order: number;
  frequency: number | null;
  /** single-sided peak amplitude in the vector's unit */
  amplitude: number | null;
  /** radians */
  phase: number | null;
}
/** A `.four` result. */
export interface Fourier {
  vector: string;
  unit: string;
  fundamental: number | null;
  dc: number | null;
  /** total harmonic distortion as a fraction (C prints 100 × thd %) */
  thd: number | null;
  harmonics: Harmonic[];
}
export interface SimResult {
  /** exactly one plot: the deck's analysis, restricted to its .save/.print vectors */
  plots: Plot[];
  /** the .print table, when the deck has a .print card */
  printed?: string | null;
  measurements?: Measurement[];
  fourier?: Fourier[];
}

export type WorkerIn =
  | { type: "init" }
  | { type: "run"; id: number; deck: string; names: string[]; contents: string[] };
export type WorkerOut =
  | { type: "ready"; version: string }
  | { type: "fatal"; message: string }
  | { type: "result"; id: number; json: string; ms: number }
  | { type: "error"; id: number; message: string };
