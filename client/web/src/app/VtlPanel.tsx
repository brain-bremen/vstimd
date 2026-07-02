// Virtual trigger lines as clickable per-bank binary views. Reads the live
// per-bit state from snapshot.vtlLines (the server lists every bit, named or
// not) and toggles a line on click: inputs simulate the hardware bridge, outputs
// override the animation-driven level — both for debugging.

import { VtlHandle } from "../index.js";
import type { Connection, SceneSnapshot, VtlKind, VtlLineView } from "../index.js";

interface Props {
  conn: Connection | null;
  snapshot: SceneSnapshot | null;
}

interface Bank {
  kind: VtlKind;
  bank: number;
  bits: (VtlLineView | undefined)[]; // indexed by bit
}

function groupBanks(lines: VtlLineView[]): Bank[] {
  const map = new Map<string, Bank>();
  for (const l of lines) {
    const key = `${l.kind}:${l.bank}`;
    let g = map.get(key);
    if (!g) {
      g = { kind: l.kind, bank: l.bank, bits: [] };
      map.set(key, g);
    }
    g.bits[l.bit] = l;
  }
  return [...map.values()].sort(
    (a, b) => a.kind.localeCompare(b.kind) || a.bank - b.bank,
  );
}

export function VtlPanel({ conn, snapshot }: Props) {
  const banks = groupBanks(snapshot?.vtlLines ?? []);

  function toggle(kind: VtlKind, bank: number, bit: number) {
    if (!conn) return;
    const handle =
      kind === "input" ? VtlHandle.input(bank, bit) : VtlHandle.output(bank, bit);
    void conn.vtl.toggleLine(handle);
  }

  return (
    <div style={{ minWidth: 280 }}>
      <h3>Trigger Lines</h3>
      {banks.length === 0 && <p style={{ color: "#666", fontSize: 13 }}>No VTL banks.</p>}
      {banks.map((g) => {
        const width = g.bits.length || 64;
        // MSB-first (bit width-1 … 0), grouped into bytes of 8.
        const idxs = Array.from({ length: width }, (_, i) => width - 1 - i);
        return (
          <div key={`${g.kind}:${g.bank}`} style={{ marginBottom: 8 }}>
            <div style={{ fontSize: 12, color: "#888", marginBottom: 2 }}>
              {g.kind === "input" ? "In" : "Out"} bank {g.bank}
            </div>
            <div style={{ fontFamily: "monospace", fontSize: 13, lineHeight: 1.6 }}>
              {idxs.map((bit, i) => {
                const line = g.bits[bit];
                const high = line?.high ?? false;
                const named = !!line?.name;
                return (
                  <span key={bit}>
                    <span
                      role="button"
                      title={`${g.kind} bank ${g.bank} bit ${bit}${named ? `: ${line!.name}` : ""}`}
                      onClick={() => toggle(g.kind, g.bank, bit)}
                      style={{
                        cursor: conn ? "pointer" : "default",
                        color: high ? "#1a1a1a" : "#777",
                        background: high ? "#4c8" : "transparent",
                        textDecoration: named ? "underline" : "none",
                        padding: "0 1px",
                        borderRadius: 2,
                      }}
                    >
                      {high ? "1" : "0"}
                    </span>
                    {i % 8 === 7 && i !== idxs.length - 1 ? " " : ""}
                  </span>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}
