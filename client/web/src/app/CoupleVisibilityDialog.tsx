// "Couple visibility to trigger line" animation dialog. Mirrors stimulus
// enabled-state to a VTL input level.
//
// TODO: replace with a JSON-Schema-driven dialog once the config schema lands
// (PLAN step 1/2) — the trigger/stimuli/polarity fields here are hand-written
// and cover only this one animation type.

import { useState } from "react";
import { VtlHandle } from "../index.js";
import type { Connection, SceneSnapshot, StimulusHandle, VtlDirection } from "../index.js";
import { Dialog, Field, NumberInput } from "./Dialog.js";

interface Props {
  conn: Connection;
  snapshot: SceneSnapshot | null;
  defaultName: string;
  onClose: () => void;
}

export function CoupleVisibilityDialog({ conn, snapshot, defaultName, onClose }: Props) {
  const stimuli = snapshot?.stimuli ?? [];
  // Named VTL lines, to pick a trigger from (matches the egui overlay).
  // Selecting a line fills in its bank/bit *and* direction.
  const namedLines = (snapshot?.vtlLines ?? []).filter((l) => l.name);
  const [name, setName] = useState(defaultName);
  const [direction, setDirection] = useState<VtlDirection>("input");
  const [bank, setBank] = useState(0);
  const [bit, setBit] = useState(0);
  const [polarity, setPolarity] = useState(true);
  const [selected, setSelected] = useState<StimulusHandle[]>([]);

  function pickNamedLine(value: string) {
    if (!value) return;
    const [dir, b, i] = value.split(":");
    setDirection(dir as VtlDirection);
    setBank(Number(b));
    setBit(Number(i));
  }

  function toggleStimulus(handle: StimulusHandle, on: boolean) {
    setSelected((prev) => (on ? [...prev, handle] : prev.filter((h) => h !== handle)));
  }

  async function submit() {
    const trigger =
      direction === "input" ? VtlHandle.input(bank, bit) : VtlHandle.output(bank, bit);
    await conn.animations.coupleVisibilityToTriggerLine(trigger, selected, { name, polarity });
    onClose();
  }

  return (
    <Dialog title="Couple visibility to trigger line" onClose={onClose} onSubmit={submit}>
      <Field label="Name">
        <input value={name} onChange={(e) => setName(e.target.value)} style={{ width: "100%" }} />
      </Field>
      {namedLines.length > 0 && (
        <Field label="Named line">
          <select
            value={`${direction}:${bank}:${bit}`}
            onChange={(e) => pickNamedLine(e.target.value)}
          >
            <option value="">— pick a named line —</option>
            {namedLines.map((l) => (
              <option key={`${l.direction}:${l.bank}:${l.bit}`} value={`${l.direction}:${l.bank}:${l.bit}`}>
                {l.name} ({l.direction} {l.bank}/{l.bit})
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field label="Direction">
        <select value={direction} onChange={(e) => setDirection(e.target.value as VtlDirection)}>
          <option value="input">input</option>
          <option value="output">output</option>
        </select>
      </Field>
      <Field label="Trigger bank, bit">
        <div style={{ display: "flex", gap: 6 }}>
          <NumberInput value={bank} onChange={setBank} step={1} />
          <NumberInput value={bit} onChange={setBit} step={1} />
        </div>
      </Field>
      <div style={{ fontSize: 11, color: "#888", gridColumn: "1 / -1", marginTop: -4 }}>
        Trigger is read from the {direction} line at this bank/bit.
      </div>
      <Field label="Polarity">
        <label style={{ fontSize: 13 }}>
          <input type="checkbox" checked={polarity} onChange={(e) => setPolarity(e.target.checked)} />{" "}
          visible when line high
        </label>
      </Field>
      <div style={{ fontSize: 13 }}>
        <div style={{ color: "#aaa", marginBottom: 4 }}>Stimuli</div>
        {stimuli.length === 0 ? (
          <p style={{ color: "#666" }}>No stimuli to couple.</p>
        ) : (
          <div style={{ display: "grid", gap: 2, maxHeight: 160, overflowY: "auto" }}>
            {stimuli.map((s) => (
              <label key={s.handle} style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <input
                  type="checkbox"
                  checked={selected.includes(s.handle)}
                  onChange={(e) => toggleStimulus(s.handle, e.target.checked)}
                />
                {s.name || <em>—</em>} <span style={{ color: "#888" }}>({s.kind})</span>
              </label>
            ))}
          </div>
        )}
      </div>
    </Dialog>
  );
}
