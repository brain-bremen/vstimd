PREFIX      ?= /usr
UNITDIR     ?= /lib/systemd/system
SYSUSERSDIR ?= /usr/lib/sysusers.d
BINARY      := target/release/vstimd
SERVICE     := packaging/systemd/vstimd.service
TARGET_UNIT := packaging/systemd/vstimd.target
BOOT_SCRIPT := packaging/scripts/vstimd-boot-entry
SYSUSERS    := packaging/sysusers/vstimd.conf

DIST_DIR          ?= dist
DEB_BUILDER_IMAGE ?= vstimd-deb-builder
RPM_BUILDER_IMAGE ?= vstimd-rpm-builder

VERSION  := $(shell grep '^version' server/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
REVISION ?= 1

DEB_AMD64 := $(DIST_DIR)/vstimd_$(VERSION)-$(REVISION)_amd64.deb
DEB_ARM64 := $(DIST_DIR)/vstimd_$(VERSION)-$(REVISION)_arm64.deb
RPM_AMD64 := $(DIST_DIR)/vstimd-$(VERSION)-$(REVISION).x86_64.rpm
RPM_ARM64 := $(DIST_DIR)/vstimd-$(VERSION)-$(REVISION).aarch64.rpm

RUST_SRCS     := Cargo.toml Cargo.lock $(shell find server/src vtl/src proto -type f 2>/dev/null)
PKG_SRCS      := $(shell find packaging -type f)

.PHONY: build install uninstall setup-user \
        deb-amd64 deb-arm64 deb \
        rpm-amd64 rpm-arm64 rpm \
        packages

build:
	cargo build --release

install:
	install -D -m 0755 $(BINARY)      $(DESTDIR)$(PREFIX)/bin/vstimd
	install -D -m 0755 $(BOOT_SCRIPT) $(DESTDIR)$(PREFIX)/sbin/vstimd-boot-entry
	install -D -m 0644 $(SERVICE)     $(DESTDIR)$(UNITDIR)/vstimd.service
	install -D -m 0644 $(TARGET_UNIT) $(DESTDIR)$(UNITDIR)/vstimd.target
	install -D -m 0644 $(SYSUSERS)    $(DESTDIR)$(SYSUSERSDIR)/vstimd.conf

uninstall:
	systemctl disable --now vstimd 2>/dev/null || true
	vstimd-boot-entry --remove 2>/dev/null || true
	rm -f $(DESTDIR)$(PREFIX)/bin/vstimd
	rm -f $(DESTDIR)$(PREFIX)/sbin/vstimd-boot-entry
	rm -f $(DESTDIR)$(UNITDIR)/vstimd.service
	rm -f $(DESTDIR)$(UNITDIR)/vstimd.target
	rm -f $(DESTDIR)$(SYSUSERSDIR)/vstimd.conf
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
