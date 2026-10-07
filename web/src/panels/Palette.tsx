import { clsx } from "clsx";
import type { Kind, ModelType } from "../circuit/model.ts";
import { KIND_LABEL, MODEL_TYPE_LABEL } from "../circuit/model.ts";
import { symbolBody } from "../circuit/symbols.tsx";
import type { Tool } from "../editor/Canvas.tsx";

interface Part {
  kind: Kind;
  modelType?: ModelType;
}
const PASSIVES: Part[] = ["R", "C", "L", "V", "I"].map((kind) => ({ kind: kind as Kind }));
const SEMIS: Part[] = [
  { kind: "D", modelType: "d" },
  { kind: "Q", modelType: "npn" },
  { kind: "Q", modelType: "pnp" },
  { kind: "M", modelType: "nmos" },
  { kind: "M", modelType: "pmos" },
];
const MARKERS: Part[] = [{ kind: "GND" }, { kind: "LABEL" }];
const partLabel = (p: Part): string =>
  p.modelType && p.kind !== "D" ? `${MODEL_TYPE_LABEL[p.modelType]} ${p.kind === "Q" ? "BJT" : "MOSFET"}` : KIND_LABEL[p.kind];

function Icon({ kind, modelType }: { kind: Kind; modelType?: ModelType }) {
  const vb = kind === "GND" ? "-16 -4 32 28" : kind === "LABEL" ? "-4 -12 50 24" : "-34 -34 68 68";
  return (
    <svg viewBox={vb} className="h-8 w-10 shrink-0">
      <g className="sym">{symbolBody(kind, modelType)}</g>
      {kind === "LABEL" && (
        <text x={8} y={4} className="fill-accent text-[12px] font-semibold">
          net
        </text>
      )}
    </svg>
  );
}

export function Palette({ tool, setTool }: { tool: Tool; setTool: (t: Tool) => void }) {
  const item = (active: boolean, onClick: () => void, children: React.ReactNode, title: string) => (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className={clsx(
        "flex w-full items-center gap-1.5 rounded border px-1.5 py-1 text-left text-xs hover:bg-hover",
        active ? "border-accent bg-hover text-accent" : "border-transparent",
      )}
    >
      {children}
    </button>
  );
  return (
    <div className="flex h-full w-36 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-line bg-panel p-1.5">
      <div className="px-1 pb-0.5 text-[10px] font-semibold tracking-wide text-muted uppercase">Tools</div>
      {item(tool.type === "select", () => setTool({ type: "select" }), <><span className="w-10 text-center">↖</span> Select <kbd className="ml-auto text-muted">S</kbd></>, "Select / move (S)")}
      {item(tool.type === "wire", () => setTool({ type: "wire" }), <><span className="w-10 text-center">⌇</span> Wire <kbd className="ml-auto text-muted">W</kbd></>, "Draw wires (W)")}
      {item(tool.type === "probe", () => setTool({ type: "probe" }), <><span className="w-10 text-center">◎</span> Probe <kbd className="ml-auto text-muted">P</kbd></>, "Click nets to plot v(net) (P)")}
      {(
        [
          ["Parts", PASSIVES],
          ["Semiconductors", SEMIS],
          ["Nets", MARKERS],
        ] as const
      ).map(([heading, parts]) => (
        <div key={heading} className="flex flex-col gap-0.5">
          <div className="px-1 pt-2 pb-0.5 text-[10px] font-semibold tracking-wide text-muted uppercase">{heading}</div>
          {parts.map((p) => (
            <div key={`${p.kind}${p.modelType ?? ""}`}>
              {item(
                tool.type === "place" && tool.kind === p.kind && tool.modelType === p.modelType,
                () => setTool({ type: "place", kind: p.kind, modelType: p.modelType }),
                <>
                  <Icon kind={p.kind} modelType={p.modelType} />
                  <span className="leading-tight">{partLabel(p)}</span>
                </>,
                `Place ${partLabel(p)}`,
              )}
            </div>
          ))}
        </div>
      ))}
      <div className="mt-auto px-1 pt-2 text-[10px] leading-snug text-muted">
        R rotates · Del removes · Ctrl+Z undo · wheel zooms · space-drag pans
      </div>
    </div>
  );
}
