# Deployment

vstimd is designed to run as a systemd service on bare-metal Linux, driving the
display directly via `VK_KHR_display` without a compositor.

## Supported platforms

| Platform | OS | Notes |
|---|---|---|
| Jetson Orin (Tegra) | Ubuntu (L4T) | Primary target; GPU and display controller are separate DRM nodes |
| Raspberry Pi 4 / 5 | Raspberry Pi OS | Full KMS overlay required; see below |
| x86 / desktop NVIDIA | Ubuntu | Extra kernel parameter required; see below |

---

## Manual install (any distro)

The repo ships a `Makefile` with a `DESTDIR`-aware `install` target — the same
target used by the `.deb` and `.rpm` packaging backends.

```bash
# 1. Build
cargo build --release          # or: make build

# 2. Install files and create the vstimd system user
sudo make install              # → /usr/bin/vstimd, /usr/lib/systemd/system/, /usr/lib/sysusers.d/
sudo make setup-user           # runs systemd-sysusers to create the vstimd user/groups

# 3. Enable and start
sudo systemctl daemon-reload
sudo systemctl enable --now vstimd
```

### Makefile targets

| Target | Effect |
|---|---|
| `make build` | `cargo build --release` |
| `make install` | Install binary, unit file, and sysusers conf to `$(DESTDIR)$(PREFIX)/…` |
| `make uninstall` | Stop, disable, and remove all installed files |
| `make setup-user` | Create the `vstimd` system user via `systemd-sysusers` |

Override defaults with variables:

```bash
sudo make install PREFIX=/usr/local UNITDIR=/usr/local/lib/systemd/system
```

### User and group provisioning

`make install` places `packaging/sysusers/vstimd.conf` in `/usr/lib/sysusers.d/`.
`make setup-user` then calls `systemd-sysusers` to create the `vstimd` system user
and add it to the `input`, `video`, and `render` groups.  Package installs (`.deb`,
`.rpm`) run this step automatically in their post-install hooks.

---

## Common setup (all platforms)

### 1. Display manager

vstimd acquires the display via `VK_KHR_display`, which requires DRM master on the
VT it is configured to use (`TTYPath=/dev/tty1` in the unit file).

**Dedicated / headless hardware (recommended):** disable the display manager
entirely so nothing contends for the display.

```bash
# Ubuntu / L4T
sudo systemctl disable --now gdm

# Raspberry Pi OS
sudo systemctl disable --now lightdm
# or via raspi-config → System Options → Boot → Console (no desktop)
```

**Desktop / development machine:** VT switching allows coexistence.  vstimd
runs on VT3 by default (`TTYPath=/dev/tty3`).  Ctrl+Alt+F1–F12 is intercepted
and forwarded so you can switch back to your desktop session; the input grab is
released while vstimd is in the background.  The unit file strips `DISPLAY`,
`WAYLAND_DISPLAY`, and `XDG_RUNTIME_DIR` from the environment (`UnsetEnvironment`)
so Vulkan does not fall back to WSI.

### 2. Groups

The `vstimd` user needs:

| Group | Device | Notes |
|---|---|---|
| `input` | `/dev/input/event*` | libinput keyboard/mouse |
| `video` | `/dev/dri/card*` | DRM master / Vulkan |
| `render` | `/dev/dri/renderD*` | GPU nodes on Raspberry Pi OS |

These are added automatically by `make setup-user` / package post-install.
For an existing login user running vstimd directly (development only):

```bash
sudo usermod -aG input,video,render $USER
# log out and back in for group changes to take effect
```

---

## Platform-specific notes

### Jetson Orin (Tegra / L4T)

The Orin has a split DRM architecture:

| DRM node | Driver | Role |
|---|---|---|
| `card0` / `renderD128` | `nvgpu` (`13e00000.host1x`) | GPU — Vulkan runs here |
| `card1` / `renderD129` | display controller (`13800000.display`) | KMS/scanout |

Because the Vulkan device and the display controller are different hardware nodes,
`VK_EXT_acquire_drm_display` does not work.  `VK_KHR_display` works: the Vulkan
driver enumerates displays directly without a DRM fd.

No special kernel parameters are required.  The display controller driver loads
from the device tree at boot.

### Raspberry Pi 4 / 5

The Pi display stack requires **full KMS** (not fake-KMS) for `VK_KHR_display`.
Add or confirm this line in `/boot/firmware/config.txt` (Pi OS Bookworm) or
`/boot/config.txt` (older):

```
dtoverlay=vc4-kms-v3d
```

The `vc4-fkms-v3d` overlay (fake KMS) is **not** sufficient.

After changing the overlay, reboot and verify:

```bash
# Should list a card with connected displays
ls /dev/dri/
cat /sys/class/drm/card*/status
```

The Vulkan driver (`v3d`) and display controller (`vc4`) are again separate DRM
nodes on Pi 4/5, similar to Jetson.

### Desktop / workstation NVIDIA (proprietary driver)

The `nvidia-drm` module must have KMS enabled.  Add to the kernel command line:

```
nvidia-drm.modeset=1
```

**Ubuntu with GRUB:**

```bash
# /etc/default/grub
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash nvidia-drm.modeset=1"

sudo update-grub
sudo reboot
```

Verify after reboot:

```bash
cat /sys/module/nvidia_drm/parameters/modeset   # should print Y
```

Without `modeset=1`, `VK_KHR_display` will find no displays and fail at startup.

---

## Boot to vstimd

vstimd ships a custom `vstimd.target` that sits between `multi-user.target` and
`graphical.target`.  Booting into it starts vstimd (and everything else in
`multi-user.target` — networking, logging, etc.) without starting a display
manager.  A normal boot into `graphical.target` leaves vstimd alone.

**Setup (once):**

```bash
# Enable vstimd to start when vstimd.target is reached.
# This does NOT make it start on normal graphical boots.
sudo systemctl enable vstimd
```

### GRUB (x86 — Fedora, Ubuntu, Debian)

**Fedora — use `grubby`** (easiest, version-independent):

```bash
sudo grubby --copy-default \
  --add-kernel=$(grubby --default-kernel) \
  --title="Boot to vstimd" \
  --args="systemd.unit=vstimd.target"

# Verify the new entry was added:
sudo grubby --info=ALL | grep -A4 "vstimd"
```

**Ubuntu / Debian — add a custom entry manually:**

Find your current kernel and initrd paths:

```bash
# or just look in /boot:
ls /boot/vmlinuz-* /boot/initrd.img-* | sort | tail -2
```

Then add the entry to `/etc/grub.d/40_custom`:

```
menuentry "Boot to vstimd" {
    # Copy the linux/initrd lines from your default entry in /boot/grub/grub.cfg,
    # then append systemd.unit=vstimd.target to the linux line.
    # Example (adjust paths to your kernel version):
    load_video
    set gfxpayload=keep
    linux   /boot/vmlinuz-6.8.0-51-generic root=UUID=<your-root-uuid> ro quiet systemd.unit=vstimd.target
    initrd  /boot/initrd.img-6.8.0-51-generic
}
```

Apply:

```bash
sudo update-grub          # Debian/Ubuntu
# or:
sudo grub2-mkconfig -o /boot/grub2/grub.cfg          # Fedora (BIOS)
sudo grub2-mkconfig -o /boot/efi/EFI/fedora/grub.cfg # Fedora (UEFI)
```

### extlinux (Jetson, Raspberry Pi, embedded)

Edit `/boot/extlinux/extlinux.conf`.  Copy your primary entry and add
`systemd.unit=vstimd.target` to the `APPEND` line:

```
LABEL vstimd
    MENU LABEL Boot to vstimd
    LINUX /boot/Image
    INITRD /boot/initrd
    APPEND ${cbootargs} quiet root=PARTUUID=<your-partuuid> rw systemd.unit=vstimd.target
```

No rebuild step is needed — extlinux reads the file directly at boot.

On Jetson the file is typically at `/boot/extlinux/extlinux.conf`; on Raspberry
Pi it may be at `/boot/firmware/extlinux/extlinux.conf` (Pi OS Bookworm) or
`/boot/extlinux/extlinux.conf` (older).

### Switching back to the desktop

Select your normal boot entry from the GRUB/extlinux menu.  Or, if you booted
into vstimd and want to start the desktop without rebooting:

```bash
# Stop vstimd, then bring up the full graphical session:
sudo systemctl stop vstimd
sudo systemctl isolate graphical.target
```

> **Note:** Runtime VT switching (Ctrl+Alt+Fn while vstimd is running) currently
> works only when vstimd was started from a TTY session, not from an X11/Wayland
> desktop.  When using the "Boot to vstimd" entry, switching works correctly.

---

## Packaging

### Installed file layout

| Source | Installed path |
|---|---|
| `target/release/vstimd` | `/usr/bin/vstimd` |
| `packaging/systemd/vstimd.service` | `/usr/lib/systemd/system/vstimd.service` |
| `packaging/sysusers/vstimd.conf` | `/usr/lib/sysusers.d/vstimd.conf` |

Both `.deb` and `.rpm` backends delegate file installation to `make install` via
`DESTDIR`, so the layout above is always consistent.

### Build the .deb (Docker — recommended)

```bash
# 1. Build the binary.
cargo build --release

# 2. Build the .deb inside a container (no packaging tools needed on host).
docker build -f packaging/docker/Dockerfile.deb-builder -t vstimd-deb-builder .
docker run --rm -v $(pwd)/packaging:/output vstimd-deb-builder
# packaging/vstimd_0.1.0-1_amd64.deb is ready
```

### Build the .deb (native)

Requires `debhelper` >= 13 and `dpkg-dev` on the host.
`dpkg-buildpackage` expects `debian/` at the repo root, so symlink it first:

```bash
cargo build --release
ln -sf packaging/debian debian
dpkg-buildpackage -b --no-sign
rm debian
# ../vstimd_0.1.0-1_amd64.deb
```

### Cross-compile for arm64 (Jetson / Raspberry Pi)

```bash
sudo apt install gcc-aarch64-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu

# Docker build picks up the cross-compiled binary automatically via debian/rules.
docker build -f packaging/docker/Dockerfile.deb-builder -t vstimd-deb-builder .
docker run --rm -v $(pwd)/packaging:/output vstimd-deb-builder
```

### Install on target

```bash
sudo dpkg -i vstimd_0.1.0-1_arm64.deb
sudo systemctl enable --now vstimd
```

`postinst` calls `systemd-sysusers` to create the `vstimd` user, and warns if a
display manager is enabled on the same VT.

---

## Docker integration test

Tests the full install + systemd lifecycle using the null renderer (no GPU
required). Requires Docker with cgroup v2 support and the `.deb` already built.

```bash
# 1. Build binary and .deb
cargo build --release
docker build -f packaging/docker/Dockerfile.deb-builder -t vstimd-deb-builder .
docker run --rm -v $(pwd)/packaging:/output vstimd-deb-builder

# 2. Build the test image
docker build -f packaging/docker/Dockerfile.test-deb -t vstimd-test-deb .

# 3. Run the test (privileged required for systemd)
packaging/docker/run-test.sh
```

The test script (`packaging/docker/test-service.sh`) exercises:
1. `dpkg -i` — package installs cleanly
2. `systemctl start vstimd` — `Type=notify` handshake succeeds within 20 s
3. ZMQ port 5555 is reachable
4. `systemctl stop vstimd` — clean SIGTERM shutdown
5. No zombie process after stop

---

## Roadmap

### RPM / Fedora packaging

`packaging/rpm/vstimd.spec` exists and uses the same `make install DESTDIR=…`
backend as the `.deb`.  Building it requires a pre-built binary and `rpmbuild`:

```bash
cargo build --release
rpmbuild -bb packaging/rpm/vstimd.spec \
    --define "_sourcedir $(pwd)/target/release"
```

A Docker-based `.rpm` builder (mirroring `Dockerfile.deb-builder`) and a Fedora
integration test are planned but not yet implemented.

### CI packaging

The GitHub Actions pipeline currently builds, tests, and runs the null-renderer
e2e suite.  Planned additions:

- **`make install` smoke test** — verify the Makefile install target places files
  correctly using `DESTDIR`, without requiring a real system install.
- **`.deb` build job** — run the Docker deb builder on every PR and upload the
  package as an artifact.
- **`.rpm` build job** — same, once the RPM Docker builder exists.
- **Integration test job** — run `packaging/docker/run-test.sh` in CI to catch
  regressions in the full install + systemd lifecycle.
