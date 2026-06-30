# vstimd web client — build, codegen, dev, and tests.
# Mirrors client/python/Makefile in spirit (use these targets, not raw commands).

SERVER_ROOT := ../..
# Window size for `make dev` (override: make dev WINDOW=1920x1080)
WINDOW ?= 1280x720

.PHONY: install gen dev dev-null build typecheck test-e2e test-ui test clean

# Install node deps if missing.
node_modules:
	npm install

install:
	npm install

# Regenerate the protobuf-es client from ../../proto.
gen: node_modules
	npm run gen

# One-liner manual test: start the REAL (windowed) server + the Vite dev server
# together. The browser UI is at http://localhost:5173 (proxies /ws + /events to
# the server on :8080). Ctrl-C stops both.
dev: gen
	cargo build --manifest-path $(SERVER_ROOT)/Cargo.toml --release --bin vstimd
	@$(MAKE) --no-print-directory _run SERVER_ARGS="--windowed $(WINDOW)"

# Same, but headless (null renderer) — no display window, web UI only.
dev-null: gen
	cargo build --manifest-path $(SERVER_ROOT)/Cargo.toml --release --bin vstimd
	@$(MAKE) --no-print-directory _run SERVER_ARGS="--null"

# Internal: run the prebuilt server + Vite dev together; Ctrl-C stops both.
# Runs the binary directly (not via `cargo run`) so we hold its PID, and kills
# children explicitly rather than the whole process group (which would also
# signal make itself).
.PHONY: _run
_run:
	@echo "vstimd: starting server + web dev UI (Ctrl-C stops both)…"
	@$(SERVER_ROOT)/target/release/vstimd $(SERVER_ARGS) & server=$$!; \
	npm run dev -- --host 127.0.0.1 & ui=$$!; \
	trap "kill -INT $$server 2>/dev/null; kill -TERM $$ui 2>/dev/null" INT TERM; \
	wait $$ui; kill -INT $$server 2>/dev/null; wait $$server 2>/dev/null || true

build: gen
	npm run build

typecheck: gen
	npx tsc --noEmit

# Node WebSocket e2e against a self-spawned `vstimd --null` (isolated ports).
test-e2e: gen
	npm run test:e2e

# Playwright browser e2e (needs Chromium: `npx playwright install chromium`).
test-ui: gen
	npm run test:ui

test: typecheck test-e2e

clean:
	rm -rf node_modules dist src/_proto test-results playwright-report
