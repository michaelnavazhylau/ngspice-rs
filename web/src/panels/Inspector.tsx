import type { ReactNode } from "react";
import type { Component, Doc, ModelSpec, ModelType, Waveform } from "../circuit/model.ts";
import { DEFAULT_MODEL, KIND_LABEL, MODEL_TYPE_LABEL, MODEL_TYPES, modelFor } from "../circuit/model.ts";
import { rotateIds, updateComponent } from "../editor/ops.ts";

interface Props {
  doc: Doc;
  selection: ReadonlySet<string>;
  commit: (d: Doc, key?: string) => void;
}

function Field({ label, value, onChange, placeholder }: { label: string; value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <label className="block">
      <span className="label">{label}</span>
      <input className="field" value={value} placeholder={placeholder} spellCheck={false} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

export function Inspector({ doc, selection, commit }: Props) {
  const comps = doc.components.filter((c) => selection.has(c.id));
  const nWires = doc.wires.filter((w) => selection.has(w.id)).length;
  if (comps.length === 0) {
    return (
      <p className="p-3 text-xs text-muted">
        {nWires ? `${nWires} wire segment${nWires > 1 ? "s" : ""} selected.` : "Select a component to edit its name and value."}
      </p>
    );
  }
  if (comps.length > 1 || nWires) {
    return (
      <div className="p-3 text-xs text-muted">
        {comps.length} components{nWires ? ` and ${nWires} wires` : ""} selected.
        <div className="mt-2">
          <button className="btn" onClick={() => commit(rotateIds(doc, selection))}>
            Rotate (R)
          </button>
        </div>
      </div>
    );
  }
  return <ComponentEditor c={comps[0]!} doc={doc} commit={commit} />;
}

function ComponentEditor({ c, doc, commit }: { c: Component; doc: Doc; commit: (d: Doc, key?: string) => void }) {
  const set = (patch: Partial<Component>, field: string) => commit(updateComponent(doc, c.id, patch), `${c.id}:${field}`);
  const src = c.src;
  const setSrc = (patch: Partial<NonNullable<Component["src"]>>, field: string) => src && set({ src: { ...src, ...patch } }, field);
  const dup = c.kind !== "GND" && c.kind !== "LABEL" && doc.components.some((o) => o.id !== c.id && o.name.toLowerCase() === c.name.toLowerCase());
  let valueField: ReactNode = null;
  if (c.kind === "R" || c.kind === "C" || c.kind === "L") {
    valueField = (
      <Field
        label={`Value (${c.kind === "R" ? "Ω" : c.kind === "C" ? "F" : "H"}) – e.g. 1k, 10u, 1meg, {param}`}
        value={c.value}
        onChange={(v) => set({ value: v }, "value")}
      />
    );
  }
  if (c.model) {
    valueField = <ModelFields c={c} model={c.model} doc={doc} commit={commit} />;
  }
  return (
    <div className="space-y-2.5 p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-semibold">{KIND_LABEL[c.kind]}</h3>
        <button className="btn" onClick={() => commit({ ...doc, components: doc.components.map((x) => (x.id === c.id ? { ...x, rot: (((x.rot + 1) % 4) as Component["rot"]) } : x)) })}>
          Rotate (R)
        </button>
      </div>
      {c.kind === "LABEL" ? (
        <Field label="Net name (lowercase letters, digits, _; “0” = ground)" value={c.value} onChange={(v) => set({ value: v }, "value")} />
      ) : c.kind === "GND" ? (
        <p className="text-xs text-muted">Connects its net to node 0, the reference.</p>
      ) : (
        <>
          <Field label="Name" value={c.name} onChange={(v) => set({ name: v }, "name")} />
          {dup && <p className="text-[11px] text-danger">Another component already uses this name.</p>}
          {valueField}
        </>
      )}
      {src && (
        <div className="space-y-2">
          <label className="block">
            <span className="label">Waveform</span>
            <select className="field" value={src.wave} onChange={(e) => setSrc({ wave: e.target.value as Waveform }, "wave")}>
              <option value="dc">DC</option>
              <option value="pulse">PULSE</option>
              <option value="pwl">PWL</option>
            </select>
          </label>
          {src.wave === "dc" && <Field label={c.kind === "V" ? "DC value (V)" : "DC value (A)"} value={src.dc} onChange={(v) => setSrc({ dc: v }, "dc")} />}
          {src.wave === "pulse" && (
            <div className="grid grid-cols-2 gap-2">
              {(
                [
                  ["v1", "Initial (v1)"],
                  ["v2", "Pulsed (v2)"],
                  ["td", "Delay (td)"],
                  ["tr", "Rise (tr)"],
                  ["tf", "Fall (tf)"],
                  ["pw", "Width (pw)"],
                  ["per", "Period (per)"],
                ] as const
              ).map(([k, label]) => (
                <Field key={k} label={label} value={src.pulse[k]} onChange={(v) => setSrc({ pulse: { ...src.pulse, [k]: v } }, `pulse${k}`)} />
              ))}
            </div>
          )}
          {src.wave === "pwl" && (
            <label className="block">
              <span className="label">Points: t1 v1 t2 v2 …</span>
              <textarea className="field h-16" value={src.pwl} spellCheck={false} onChange={(e) => setSrc({ pwl: e.target.value }, "pwl")} />
            </label>
          )}
          <Field label="AC magnitude (blank = none)" value={src.ac} placeholder="1" onChange={(v) => setSrc({ ac: v }, "ac")} />
        </div>
      )}
    </div>
  );
}

const sameModel = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

const INSTANCE_LABEL: Partial<Record<Component["kind"], string>> = {
  D: "Area factor (optional), e.g. 2",
  Q: "Area factor (optional), e.g. 2",
  M: "Instance parameters, e.g. w=10u l=1u",
};

/** Polarity, instance parameters and the (possibly shared) model card of a D/Q/M part. */
function ModelFields({
  c,
  model,
  doc,
  commit,
}: {
  c: Component;
  model: ModelSpec;
  doc: Doc;
  commit: (d: Doc, key?: string) => void;
}) {
  const users = doc.components.filter((o) => o.model && sameModel(o.model.name, model.name));
  const setOwn = (patch: Partial<Component>, field: string) =>
    commit({ ...doc, components: doc.components.map((o) => (o.id === c.id ? { ...o, ...patch } : o)) }, `${c.id}:${field}`);
  // Model parameters belong to the card, so every part using it changes together.
  const setShared = (patch: Partial<ModelSpec>, field: string) =>
    commit(
      {
        ...doc,
        components: doc.components.map((o) =>
          o.model && sameModel(o.model.name, model.name) ? { ...o, model: { ...o.model, ...patch } } : o,
        ),
      },
      `model:${model.name.toLowerCase()}:${field}`,
    );
  const setType = (type: ModelType) => {
    // A part on its type's default card moves to the new type's default card;
    // a custom card keeps its name and parameters and only changes type.
    const onDefault = sameModel(model.name, DEFAULT_MODEL[model.type].name);
    setOwn({ model: onDefault ? modelFor(doc, type) : { ...model, type } }, "type");
  };
  const types = MODEL_TYPES[c.kind] ?? [];
  return (
    <div className="space-y-2">
      {types.length > 1 && (
        <label className="block">
          <span className="label">Polarity</span>
          <select className="field" value={model.type} onChange={(e) => setType(e.target.value as ModelType)}>
            {types.map((t) => (
              <option key={t} value={t}>
                {MODEL_TYPE_LABEL[t]}
              </option>
            ))}
          </select>
        </label>
      )}
      <Field label={INSTANCE_LABEL[c.kind] ?? "Instance parameters"} value={c.value} onChange={(v) => setOwn({ value: v }, "value")} />
      <Field label="Model name" value={model.name} onChange={(v) => setOwn({ model: { ...model, name: v } }, "modelName")} />
      <label className="block">
        <span className="label">
          .model {model.name || "?"} {model.type}( … ){users.length > 1 ? ` – shared by ${users.length} parts` : ""}
        </span>
        <textarea
          className="field h-20 font-mono"
          value={model.params}
          spellCheck={false}
          onChange={(e) => setShared({ params: e.target.value }, "params")}
        />
      </label>
      <button
        className="btn"
        onClick={() => setShared({ params: DEFAULT_MODEL[model.type].params }, "reset")}
        title="Restore the default parameters for this model type"
      >
        Default parameters
      </button>
      <p className="text-[11px] leading-snug text-muted">
        {c.kind === "M"
          ? "Level-1 MOSFET; the bulk is tied to the source. NMOS: drain on top; PMOS: source on top."
          : c.kind === "Q"
            ? "Ebers-Moll BJT with junction and transit-time charge. NPN: collector on top; PNP: emitter on top."
            : "Junction diode with series resistance and charge."}{" "}
        Parts with the same model name share one card.
      </p>
    </div>
  );
}
