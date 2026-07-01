// Minimal modal shell shared by the creation dialogs. Renders a centered card
// over a dimmed backdrop; Esc / backdrop click / Cancel all close it.
//
// TODO: these hand-written dialogs are placeholders. Once the config JSON Schema
// lands (PLAN step 1/2) the creation forms should be generated from schema so
// the fields stay in sync with the server instead of being maintained by hand.

import { useEffect, type ReactNode } from "react";

interface Props {
  title: string;
  onClose: () => void;
  onSubmit: () => void;
  submitLabel?: string;
  children: ReactNode;
}

export function Dialog({ title, onClose, onSubmit, submitLabel = "Create", children }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.6)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 100,
      }}
    >
      <form
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit();
        }}
        style={{
          background: "#222",
          color: "#ddd",
          border: "1px solid #444",
          borderRadius: 8,
          padding: 20,
          minWidth: 320,
          maxHeight: "85vh",
          overflowY: "auto",
          boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
        }}
      >
        <h3 style={{ marginTop: 0 }}>{title}</h3>
        <div style={{ display: "grid", gap: 10 }}>{children}</div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 20 }}>
          <button type="button" onClick={onClose}>Cancel</button>
          <button type="submit">{submitLabel}</button>
        </div>
      </form>
    </div>
  );
}

// Shared field helpers so the dialogs stay compact and visually consistent.

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label style={{ display: "grid", gridTemplateColumns: "110px 1fr", alignItems: "center", gap: 8, fontSize: 13 }}>
      <span style={{ color: "#aaa" }}>{label}</span>
      {children}
    </label>
  );
}

export function NumberInput({
  value,
  onChange,
  step = "any",
}: {
  value: number;
  onChange: (v: number) => void;
  step?: string | number;
}) {
  return (
    <input
      type="number"
      step={step}
      value={value}
      onChange={(e) => onChange(e.target.valueAsNumber)}
      style={{ width: "100%" }}
    />
  );
}
