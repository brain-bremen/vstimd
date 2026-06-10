---
name: run-e2e-tests
description: Run, verify, or test the vstimd server and Python client e2e tests. Use when asked to run tests, verify a change, check e2e, or run the null renderer tests.
---

E2e tests live in `client/python/tests/e2e/`. All Makefile targets run from `client/python/`. The test fixture in `test_e2e_null.py` automatically builds and starts the server in null mode if none is already running, then stops it after the suite.

## Run

```bash
# No display or GPU required — builds server if needed, starts null renderer, runs all tests
make test-e2e-null

# Requires a display (X11/Wayland) and GPU — starts server with real renderer, runs all tests
make test-e2e
```

## Gotchas

- `QueryServerInfo` returns OK with `(width, height) = (0, 0)` while the server is initialising (before the display is ready) — the fixture polls with a low-level ZMQ ping (`conftest.reachable()`), not via the Python client, to avoid acting on a zeroed response.
- Port 5555 must be free. If a previous server process leaked, find and kill it: `ss -tlnp | grep 5555`.
- `make test-e2e-null` runs `make proto` first — proto stubs are always up to date when tests run.
