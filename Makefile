PREFIX      ?= /usr
UNITDIR     ?= /lib/systemd/system
SYSUSERSDIR ?= /usr/lib/sysusers.d
CONFDIR     ?= /etc/braemons
SHAREDIR    ?= /usr/share/braemons/vstimd
BINARY      := target/release/vstimd
SERVICE     := packaging/systemd/vstimd.service
TARGET_UNIT := packaging/systemd/vstimd.target
BOOT_SCRIPT := packaging/scripts/vstimd-boot-entry
SYSUSERS    := packaging/sysusers/vstimd.conf
RIG_CONFIG  := server/config/default-rig-config.toml
EXAMPLES    := server/config/jetson-orin-nano.toml \
               server/config/raspberry-pi-5.toml \
               server/config/raspberry-pi-4.toml

DIST_DIR          ?= dist
DEB_BUILDER_IMAGE ?= vstimd-deb-builder
RPM_BUILDER_IMAGE ?= vstimd-rpm-builder

VERSION  := $(shell grep '^version' server/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
REVISION ?= 1

# Must match [package.metadata.deb] name in server/Cargo.toml
DEB_NAME  := braemons-vstimd

DEB_AMD64 := $(DIST_DIR)/$(DEB_NAME)_$(VERSION)-$(REVISION)_amd64.deb
DEB_ARM64 := $(DIST_DIR)/$(DEB_NAME)_$(VERSION)-$(REVISION)_arm64.deb
RPM_AMD64 := $(DIST_DIR)/$(DEB_NAME)-$(VERSION)-$(REVISION).x86_64.rpm
RPM_ARM64 := $(DIST_DIR)/$(DEB_NAME)-$(VERSION)-$(REVISION).aarch64.rpm

RUST_SRCS     := Cargo.toml Cargo.lock $(shell find server/src vtl/src proto -type f 2>/dev/null)
PKG_SRCS      := $(shell find packaging -type f)

WEB_DIR  := client/web
WEB_DIST := $(WEB_DIR)/dist/index.html
WEB_SRCS := $(shell find $(WEB_DIR)/src -type f 2>/dev/null) \
            $(WEB_DIR)/index.html $(WEB_DIR)/package.json $(WEB_DIR)/vite.config.ts

.PHONY: build build-server web install uninstall setup-user \
        deb-amd64 deb-arm64 deb \
        rpm-amd64 rpm-arm64 rpm \
        packages

# Build the React bundle that gets baked into the binary (requires Node/npm).
# File target so it only rebuilds when the web sources change.
$(WEB_DIST): $(WEB_SRCS)
	$(MAKE) -C $(WEB_DIR) build

web: $(WEB_DIST)

# Deployable binary WITH the browser UI embedded: serves the React app at
# http://<device>:8080 so any machine on the LAN can control vstimd. Requires
# Node/npm to build the frontend first. Use `build-server` for a UI-less build.
build: web
	cargo build --release --features embed-ui

# Server-only binary (no embedded UI, no Node/npm needed). The web control
# surface still runs, but `/` serves a placeholder instead of the React app.
build-server:
	cargo build --release

# Install a pre-built binary. Kept separate from `build` so the usual flow is
# `make build` (as your user, with cargo) then `sudo make install` (as root,
# which has no cargo/rustup in PATH). Fails clearly if the binary is missing.
install:
	@test -x $(BINARY) || { echo "error: $(BINARY) not found — run 'make build' first (as your user, not via sudo)"; exit 1; }
	@test -n "$(VSTIMD_ALLOW_NO_UI)" || grep -aq 'id="root"' $(BINARY) || { echo "error: $(BINARY) has no embedded web UI — a dev target (make dev/dev-null, cargo build/run) rebuilt it without the UI. Run 'make build' before installing, or set VSTIMD_ALLOW_NO_UI=1 to install a server-only binary."; exit 1; }
	install -D -m 0755 $(BINARY)      $(DESTDIR)$(PREFIX)/bin/vstimd
	install -D -m 0755 $(BOOT_SCRIPT) $(DESTDIR)$(PREFIX)/sbin/vstimd-boot-entry
	install -D -m 0644 $(SERVICE)     $(DESTDIR)$(UNITDIR)/vstimd.service
	install -D -m 0644 $(TARGET_UNIT) $(DESTDIR)$(UNITDIR)/vstimd.target
	install -D -m 0644 $(SYSUSERS)    $(DESTDIR)$(SYSUSERSDIR)/vstimd.conf
	install -d -m 0755 $(DESTDIR)$(CONFDIR)
	test -f $(DESTDIR)$(CONFDIR)/vstimd-rig-config.toml || \
	  install -m 0644 $(RIG_CONFIG) $(DESTDIR)$(CONFDIR)/vstimd-rig-config.toml
	install -d -m 0755 $(DESTDIR)$(SHAREDIR)
	for f in $(EXAMPLES); do install -m 0644 $$f $(DESTDIR)$(SHAREDIR)/; done

uninstall:
	systemctl disable --now vstimd 2>/dev/null || true
	vstimd-boot-entry --remove 2>/dev/null || true
	rm -f $(DESTDIR)$(PREFIX)/bin/vstimd
	rm -f $(DESTDIR)$(PREFIX)/sbin/vstimd-boot-entry
	rm -f $(DESTDIR)$(UNITDIR)/vstimd.service
	rm -f $(DESTDIR)$(UNITDIR)/vstimd.target
	rm -f $(DESTDIR)$(SYSUSERSDIR)/vstimd.conf
	for f in $(EXAMPLES); do rm -f $(DESTDIR)$(SHAREDIR)/$$(basename $$f); done
	rmdir --ignore-fail-on-non-empty $(DESTDIR)$(SHAREDIR) $(DESTDIR)$(CONFDIR) 2>/dev/null || true
	systemctl daemon-reload 2>/dev/null || true

setup-user:
	systemd-sysusers $(abspath $(SYSUSERS))

# ── Package targets (Docker-based, output to $(DIST_DIR)/) ───────────────────

deb-amd64:
	DOCKER_BUILDKIT=1 docker build \
	  -f packaging/docker/Dockerfile.deb-builder \
	  --build-arg REVISION=$(REVISION) \
	  -t $(DEB_BUILDER_IMAGE)-amd64 .
	mkdir -p $(DIST_DIR)
	docker run --rm -v $(abspath $(DIST_DIR)):/output $(DEB_BUILDER_IMAGE)-amd64

deb-arm64:
	DOCKER_BUILDKIT=1 docker build \
	  -f packaging/docker/Dockerfile.deb-builder \
	  --build-arg RUST_TARGET=aarch64-unknown-linux-gnu \
	  --build-arg DEB_HOST_ARCH=arm64 \
	  --build-arg REVISION=$(REVISION) \
	  -t $(DEB_BUILDER_IMAGE)-arm64 .
	mkdir -p $(DIST_DIR)
	docker run --rm -v $(abspath $(DIST_DIR)):/output $(DEB_BUILDER_IMAGE)-arm64

deb: deb-amd64 deb-arm64

rpm-amd64:
	DOCKER_BUILDKIT=1 docker build \
	  -f packaging/docker/Dockerfile.rpm-builder \
	  -t $(RPM_BUILDER_IMAGE)-amd64 .
	mkdir -p $(DIST_DIR)
	docker run --rm -v $(abspath $(DIST_DIR)):/output $(RPM_BUILDER_IMAGE)-amd64

rpm-arm64:
	DOCKER_BUILDKIT=1 docker build \
	  -f packaging/docker/Dockerfile.rpm-builder \
	  --build-arg RUST_TARGET=aarch64-unknown-linux-gnu \
	  --build-arg RPM_ARCH=aarch64 \
	  -t $(RPM_BUILDER_IMAGE)-arm64 .
	mkdir -p $(DIST_DIR)
	docker run --rm -v $(abspath $(DIST_DIR)):/output $(RPM_BUILDER_IMAGE)-arm64

rpm: rpm-amd64 rpm-arm64

packages: deb rpm
