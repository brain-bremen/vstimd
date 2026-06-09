"""interactive_grating.py — Live parameter editing for a grating stimulus.

Connects to a running vstimd server, creates a GratingStim, and lets you
adjust its parameters interactively from the terminal.  No extra packages
required — uses msvcrt on Windows, tty/termios on Linux/macOS.

Controls
--------
  UP / DOWN     — select parameter
  LEFT / RIGHT  — decrease / increase selected parameter
  W             — cycle waveform  (sin → sqr → saw → tri)
  M             — cycle mask      (none → circle → gauss → raisedCos → hann)
  SPACE         — toggle visibility (show / hide)
  D             — toggle server-side drift on/off
  Q / ESC       — quit and delete the stimulus

Usage
-----
    uv run examples/interactive_grating.py
    uv run examples/interactive_grating.py tcp://192.168.1.10:5555
"""

from __future__ import annotations

import argparse
import sys
import os
from dataclasses import dataclass

from vstimd.psychopy import visual
from vstimd.stimuli import GratingMask, GratingTexture


# ── Cross-platform raw key reader ─────────────────────────────────────────────

if sys.platform == "win32":
    import msvcrt

    def _getch() -> str:
        ch = msvcrt.getwch()
        if ch in ("\x00", "\xe0"):  # arrow / function key prefix
            ch2 = msvcrt.getwch()
            return ch + ch2
        return ch

    _KEY_UP    = "\xe0H"
    _KEY_DOWN  = "\xe0P"
    _KEY_LEFT  = "\xe0K"
    _KEY_RIGHT = "\xe0M"

else:
    import tty
    import termios

    def _getch() -> str:  # type: ignore[misc]
        fd = sys.stdin.fileno()
        old = termios.tcgetattr(fd)
        try:
            tty.setraw(fd)
            ch = sys.stdin.read(1)
            if ch == "\x1b":
                rest = sys.stdin.read(2)
                return ch + rest
            return ch
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old)

    _KEY_UP    = "\x1b[A"
    _KEY_DOWN  = "\x1b[B"
    _KEY_LEFT  = "\x1b[D"
    _KEY_RIGHT = "\x1b[C"


# ── ANSI helpers ──────────────────────────────────────────────────────────────

_RESET  = "\033[0m"
_BOLD   = "\033[1m"
_DIM    = "\033[2m"
_CYAN   = "\033[96m"
_YELLOW = "\033[93m"
_GREEN  = "\033[92m"
_RED    = "\033[91m"

def _ansi(*codes: str, text: str) -> str:
    return "".join(codes) + text + _RESET


def _clear() -> None:
    os.system("cls" if sys.platform == "win32" else "clear")


# ── Parameter descriptor ──────────────────────────────────────────────────────

@dataclass
class Param:
    label: str
    value: float
    step: float
    lo: float
    hi: float
    fmt: str = ".3f"

    def increase(self) -> None:
        self.value = max(self.lo, min(self.hi, self.value + self.step))

    def decrease(self) -> None:
        self.value = max(self.lo, min(self.hi, self.value - self.step))

    def display(self) -> str:
        return f"{self.value:{self.fmt}}"


# ── Cycling enum helper ───────────────────────────────────────────────────────

WAVEFORMS: list[GratingTexture]  = list(GratingTexture)
MASKS:     list[GratingMask] = list(GratingMask)


@dataclass
class CycleParam:
    label: str
    choices: list
    index: int = 0

    @property
    def value(self):  # type: ignore[return]
        return self.choices[self.index]

    def next(self) -> None:
        self.index = (self.index + 1) % len(self.choices)

    def display(self) -> str:
        return str(self.value.value)


# ── Display ───────────────────────────────────────────────────────────────────

def _render(
    address: str,
    params: list[Param],
    tex_param: CycleParam,
    mask_param: CycleParam,
    sel: int,
    visible: bool,
) -> None:
    _clear()
    title = f" vstimd — interactive grating   [{address}] "
    print(_ansi(_CYAN, _BOLD, text=title))
    print(_ansi(_DIM, text="─" * len(title)))
    print()

    for i, p in enumerate(params):
        marker = "▶" if i == sel else " "
        val = p.display()
        row = f"  {marker} {p.label:<30}  {val}"
        if i == sel:
            print(_ansi(_YELLOW, _BOLD, text=row))
        else:
            print(row)

    print()
    print(f"    {tex_param.label:<30}  {tex_param.display()}")
    print(f"    {mask_param.label:<30}  {mask_param.display()}")
    print()

    vis_text = "VISIBLE" if visible else "HIDDEN "
    vis_ansi = _ansi(_GREEN, _BOLD, text=vis_text) if visible else _ansi(_RED, _BOLD, text=vis_text)
    print(f"  autoDraw:  {vis_ansi}")
    print()
    print(_ansi(_DIM, text="  ↑↓ select   ←→ adjust   W waveform   M mask   SPACE toggle   D drift   Q quit"))


# ── Main loop ─────────────────────────────────────────────────────────────────

def run(address: str) -> None:
    win = visual.Window(
        size=(1920, 1080),
        color=(-1, -1, -1),
        units="pix",
        address=address,
        deferred=False,
    )

    grating = visual.GratingStim(
        win,
        tex=GratingTexture.SIN,
        mask=GratingMask.NONE,
        pos=(0, 0),
        size=400,
        sf=0.05,
        ori=0.0,
        phase=0.0,
        contrast=1.0,
        opacity=1.0,
        drift_speed=0.0,
        autoDraw=True,
    )

    params: list[Param] = [
        Param("sf        (cyc/px)", 0.05, 0.005, 0.001,  0.50, ".4f"),
        Param("ori       (deg)",    0.0,  5.0,  -180.0, 180.0, ".1f"),
        Param("phase     (0..1)",   0.0,  0.05,   0.0,   1.0,  ".3f"),
        Param("contrast  (0..1)",   1.0,  0.05,   0.0,   1.0,  ".3f"),
        Param("opacity   (0..1)",   1.0,  0.05,   0.0,   1.0,  ".3f"),
        Param("pos X     (px)",     0.0, 20.0, -960.0,  960.0, ".0f"),
        Param("pos Y     (px)",     0.0, 20.0, -540.0,  540.0, ".0f"),
        Param("drift spd (cyc/s)",  0.0,  0.5,  -20.0,  20.0,  ".2f"),
        Param("drift ang (deg)",    0.0,  5.0, -180.0, 180.0, ".1f"),
    ]

    tex_param  = CycleParam("waveform  (W)", WAVEFORMS, 0)
    mask_param = CycleParam("mask      (M)", MASKS,     0)

    sel = 0
    visible = True

    def apply(idx: int) -> None:
        p = params[idx]
        tag = p.label.split()[0]
        if tag == "sf":
            grating.sf = p.value
        elif tag == "ori":
            grating.ori = p.value
        elif tag == "phase":
            grating.phase = p.value
        elif tag == "contrast":
            grating.contrast = p.value
        elif tag == "opacity":
            grating.opacity = p.value
        elif tag == "pos":
            grating.pos = (params[5].value, params[6].value)
        elif tag == "drift":
            if "spd" in p.label:
                grating.drift_speed = p.value
            else:
                grating.drift_angle = p.value

    _render(address, params, tex_param, mask_param, sel, visible)

    try:
        while True:
            key = _getch()

            if key in ("q", "Q", "\x1b", "\x03"):
                break
            elif key == _KEY_UP:
                sel = (sel - 1) % len(params)
            elif key == _KEY_DOWN:
                sel = (sel + 1) % len(params)
            elif key == _KEY_RIGHT:
                params[sel].increase()
                apply(sel)
            elif key == _KEY_LEFT:
                params[sel].decrease()
                apply(sel)
            elif key in ("w", "W"):
                tex_param.next()
                win._conn.stimuli.grating.set_waveform(grating._handle, tex_param.value)
            elif key in ("m", "M"):
                mask_param.next()
                win._conn.stimuli.grating.set_mask(grating._handle, mask_param.value)
            elif key == " ":
                visible = not visible
                grating.autoDraw = visible
            elif key in ("d", "D"):
                if grating.drift_speed == 0.0:
                    params[7].value = 2.0
                    grating.drift_speed = 2.0
                else:
                    params[7].value = 0.0
                    grating.drift_speed = 0.0

            _render(address, params, tex_param, mask_param, sel, visible)

    finally:
        win._conn.stimuli.grating.delete(grating._handle)
        win.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "address",
        nargs="?",
        default="tcp://localhost:5555",
        help="ZMQ address of vstimd (default: tcp://localhost:5555)",
    )
    args = parser.parse_args()

    try:
        run(args.address)
    except KeyboardInterrupt:
        pass
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
