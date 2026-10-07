import { useState } from "react";
import type { VFile } from "../circuit/model.ts";

interface Props {
  files: VFile[];
  setFiles: (f: VFile[]) => void;
}

export function FilesPanel({ files, setFiles }: Props) {
  const [sel, setSel] = useState(0);
  const f = files[Math.min(sel, files.length - 1)];
  const idx = f ? files.indexOf(f) : -1;
  const patch = (p: Partial<VFile>) => setFiles(files.map((x, i) => (i === idx ? { ...x, ...p } : x)));
  const nameClash = f ? files.some((x, i) => i !== idx && x.name === f.name) : false;
  const add = () => {
    let n = 1;
    while (files.some((x) => x.name === `file${n}.inc`)) n++;
    setFiles([...files, { name: `file${n}.inc`, content: "* new file\n" }]);
    setSel(files.length);
  };
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-3">
      <p className="text-[11px] text-muted">
        In-memory files visible to <code>.include</code>/<code>.lib</code> (paths relative to the deck). Stored in this browser.
      </p>
      <div className="flex flex-wrap gap-1">
        {files.map((x, i) => (
          <button key={i} className={"btn font-mono " + (i === idx ? "btn-on" : "")} onClick={() => setSel(i)}>
            {x.name || "(unnamed)"}
          </button>
        ))}
        <button className="btn" onClick={add}>
          + New
        </button>
      </div>
      {f ? (
        <>
          <div className="flex items-end gap-1">
            <label className="block flex-1">
              <span className="label">File name (e.g. models/params.inc)</span>
              <input className="field" value={f.name} spellCheck={false} onChange={(e) => patch({ name: e.target.value })} />
            </label>
            <button
              className="btn text-danger"
              onClick={() => {
                setFiles(files.filter((_, i) => i !== idx));
                setSel(0);
              }}
            >
              Delete
            </button>
          </div>
          {nameClash && <p className="text-[11px] text-danger">Duplicate file name.</p>}
          <textarea
            className="field min-h-0 flex-1 resize-none"
            value={f.content}
            spellCheck={false}
            wrap="off"
            onChange={(e) => patch({ content: e.target.value })}
          />
        </>
      ) : (
        <p className="text-xs text-muted">No files yet.</p>
      )}
    </div>
  );
}
