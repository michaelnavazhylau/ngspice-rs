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
export interface SimResult {
  plots: Plot[];
}

export type WorkerIn =
  | { type: "init" }
  | { type: "run"; id: number; deck: string; names: string[]; contents: string[] };
export type WorkerOut =
  | { type: "ready"; version: string }
  | { type: "fatal"; message: string }
  | { type: "result"; id: number; json: string; ms: number }
  | { type: "error"; id: number; message: string };
