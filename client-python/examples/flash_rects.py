"""flash_rects.py — Create two rectangles and flash them alternately.

Usage
-----
    # Server must already be running:
    #   cd server && cargo run

    uv run examples/flash_rects.py                      # default tcp://localhost:5555
    uv run examples/flash_rects.py tcp://192.168.1.10:5555
    uv run examples/flash_rects.py --flashes 5 --hz 2

The two rects start disabled; the script then alternates enabling/disabling
them at --hz Hz for --flashes complete on/off cycles each, then deletes both.
"""

import argparse
import sys
import time

from wonderlamp import Connection


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "address",
        nargs="?",
        default="tcp://localhost:5555",
        help="ZMQ address of the server (default: tcp://localhost:5555)",
    )
    parser.add_argument(
        "--flashes",
        type=int,
        default=4,
        metavar="N",
        help="number of on/off cycles per rectangle (default: 4)",
    )
    parser.add_argument(
        "--hz",
        type=float,
        default=2.0,
        metavar="HZ",
        help="flash frequency in Hz (default: 2.0)",
    )
    args = parser.parse_args()

    half_period = 1.0 / (2.0 * args.hz)

    print(f"Connecting to {args.address} …")
    with Connection(args.address) as conn:
        # ── Create ────────────────────────────────────────────────────────────
        # Left rect: red, starts disabled.
        left = conn.create_rect(
            x=-200, y=0,
            width=300, height=200,
            r=0.9, g=0.15, b=0.15,
        )
        conn.set_enabled(left, False)

        # Right rect: blue, starts disabled.
        right = conn.create_rect(
            x=200, y=0,
            width=300, height=200,
            r=0.15, g=0.4, b=0.9,
        )
        conn.set_enabled(right, False)

        print(f"Created rects — handles: left={left}, right={right}")
        print(f"Flashing {args.flashes}× at {args.hz} Hz "
              f"(half-period {half_period*1000:.0f} ms) …")

        # ── Flash ─────────────────────────────────────────────────────────────
        for flash in range(args.flashes):
            # Left on, right off
            conn.set_enabled(left, True)
            conn.set_enabled(right, False)
            print(f"  flash {flash + 1}/{args.flashes}  [LEFT  ON ]", end="\r")
            time.sleep(half_period)

            # Left off, right on
            conn.set_enabled(left, False)
            conn.set_enabled(right, True)
            print(f"  flash {flash + 1}/{args.flashes}  [RIGHT ON ]", end="\r")
            time.sleep(half_period)

        # Both off at the end of the last cycle.
        conn.set_enabled(left, False)
        conn.set_enabled(right, False)
        print()  # newline after the \r progress line

        # ── Delete ────────────────────────────────────────────────────────────
        conn.delete(left)
        conn.delete(right)
        print("Done — both rects deleted.")


if __name__ == "__main__":
    try:
        main()
    except RuntimeError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nInterrupted.")
        sys.exit(0)
