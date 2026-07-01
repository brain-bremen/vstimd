// Grating creation dialog. Collects the grating parameters and calls
// conn.stimuli.grating.create.
//
// TODO: replace with a JSON-Schema-driven dialog once the config schema lands
// (PLAN step 1/2) — the field list here is maintained by hand and will drift
// from the server's grating params otherwise.

import { useState } from "react";
import type { Connection, GratingMask, Waveform } from "../index.js";
import { rgb } from "../index.js";
import { Dialog, Field, NumberInput } from "./Dialog.js";

interface Props {
  conn: Connection;
  defaultName: string;
  onClose: () => void;
}

const WAVEFORMS: Waveform[] = ["sin", "sqr", "saw", "tri"];
const MASKS: GratingMask[] = ["none", "circle", "gauss", "hann", "raisedCos"];

export function GratingDialog({ conn, defaultName, onClose }: Props) {
  const [name, setName] = useState(defaultName);
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const [width, setWidth] = useState(200);
  const [height, setHeight] = useState(200);
  const [sf, setSf] = useState(0.05);
  const [contrast, setContrast] = useState(1);
  const [phase, setPhase] = useState(0);
  const [angle, setAngle] = useState(0);
  const [driftSpeed, setDriftSpeed] = useState(0);
  const [opacity, setOpacity] = useState(1);
  const [waveform, setWaveform] = useState<Waveform>("sin");
  const [mask, setMask] = useState<GratingMask>("none");

  async function submit() {
    await conn.stimuli.grating.create({
      name,
      pos: { x, y },
      width,
      height,
      sf,
      contrast,
      phase,
      angle,
      driftSpeed,
      opacity,
      waveform,
      mask,
      foreColor: rgb(1, 1, 1),
      backColor: rgb(0, 0, 0),
    });
    onClose();
  }

  return (
    <Dialog title="Add grating" onClose={onClose} onSubmit={submit}>
      <Field label="Name">
        <input value={name} onChange={(e) => setName(e.target.value)} style={{ width: "100%" }} />
      </Field>
      <Field label="Position x, y">
        <div style={{ display: "flex", gap: 6 }}>
          <NumberInput value={x} onChange={setX} />
          <NumberInput value={y} onChange={setY} />
        </div>
      </Field>
      <Field label="Size w, h">
        <div style={{ display: "flex", gap: 6 }}>
          <NumberInput value={width} onChange={setWidth} />
          <NumberInput value={height} onChange={setHeight} />
        </div>
      </Field>
      <Field label="Spatial freq">
        <NumberInput value={sf} onChange={setSf} step={0.01} />
      </Field>
      <Field label="Contrast">
        <NumberInput value={contrast} onChange={setContrast} step={0.05} />
      </Field>
      <Field label="Phase">
        <NumberInput value={phase} onChange={setPhase} step={0.05} />
      </Field>
      <Field label="Angle (deg)">
        <NumberInput value={angle} onChange={setAngle} />
      </Field>
      <Field label="Drift speed">
        <NumberInput value={driftSpeed} onChange={setDriftSpeed} step={0.1} />
      </Field>
      <Field label="Opacity">
        <NumberInput value={opacity} onChange={setOpacity} step={0.05} />
      </Field>
      <Field label="Waveform">
        <select value={waveform} onChange={(e) => setWaveform(e.target.value as Waveform)}>
          {WAVEFORMS.map((w) => <option key={w} value={w}>{w}</option>)}
        </select>
      </Field>
      <Field label="Mask">
        <select value={mask} onChange={(e) => setMask(e.target.value as GratingMask)}>
          {MASKS.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
      </Field>
    </Dialog>
  );
}
