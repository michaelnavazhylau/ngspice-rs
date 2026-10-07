interface Props {
  generated: string;
  override: string | null;
  setOverride: (s: string | null) => void;
}

export function NetlistPanel({ generated, override, setOverride }: Props) {
  const text = override !== null;
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-3">
      <div className="flex items-center gap-1.5">
        {text ? (
          <>
            <span className="rounded bg-warn/20 px-1.5 py-0.5 text-[11px] text-warn">Text mode: the deck below is run, not the schematic</span>
            <button className="btn ml-auto" onClick={() => setOverride(null)}>
              Back to schematic
            </button>
          </>
        ) : (
          <>
            <span className="text-[11px] text-muted">Generated from the schematic (read-only)</span>
            <button className="btn ml-auto" onClick={() => setOverride(generated)}>
              Edit as text
            </button>
          </>
        )}
      </div>
      <textarea
        className="field min-h-0 flex-1 resize-none leading-snug"
        readOnly={!text}
        spellCheck={false}
        wrap="off"
        value={text ? override : generated}
        onChange={(e) => setOverride(e.target.value)}
      />
    </div>
  );
}
