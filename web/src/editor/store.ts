import { useCallback, useReducer } from "react";
import type { Doc } from "../circuit/model.ts";
import { emptyDoc } from "../circuit/model.ts";

export interface History {
  past: Doc[];
  present: Doc;
  future: Doc[];
  lastKey?: string;
  lastAt?: number;
}

type Action =
  | { type: "commit"; doc: Doc; key?: string }
  | { type: "reset"; doc: Doc }
  | { type: "undo" }
  | { type: "redo" };

const LIMIT = 200;

function reducer(h: History, a: Action): History {
  switch (a.type) {
    case "commit":
      if (a.doc === h.present) return h;
      if (a.key && h.lastKey === a.key && Date.now() - (h.lastAt ?? 0) < 1000) {
        // coalesce rapid edits of the same field into one undo step
        return { ...h, present: a.doc, future: [], lastAt: Date.now() };
      }
      return {
        past: [...h.past, h.present].slice(-LIMIT),
        present: a.doc,
        future: [],
        lastKey: a.key,
        lastAt: Date.now(),
      };
    case "reset":
      return { past: [], present: a.doc, future: [] };
    case "undo": {
      const prev = h.past[h.past.length - 1];
      if (!prev) return h;
      return { past: h.past.slice(0, -1), present: prev, future: [h.present, ...h.future] };
    }
    case "redo": {
      const next = h.future[0];
      if (!next) return h;
      return { past: [...h.past, h.present], present: next, future: h.future.slice(1) };
    }
  }
}

export function useEditorHistory(initial?: Doc) {
  const [h, dispatch] = useReducer(reducer, undefined, () => ({
    past: [],
    present: initial ?? emptyDoc(),
    future: [],
  }));
  return {
    doc: h.present,
    canUndo: h.past.length > 0,
    canRedo: h.future.length > 0,
    commit: useCallback((doc: Doc, key?: string) => dispatch({ type: "commit", doc, key }), []),
    reset: useCallback((doc: Doc) => dispatch({ type: "reset", doc }), []),
    undo: useCallback(() => dispatch({ type: "undo" }), []),
    redo: useCallback(() => dispatch({ type: "redo" }), []),
  };
}
