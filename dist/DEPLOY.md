# Wonderlamp deployment notes

Wonderlamp runs in DRM/console mode (`--mode drm`), driving the display directly
via `VK_KHR_display` without a compositor.  It is designed to replace the display
manager on the target system and run as a systemd service from boot.

## Supported platforms

| Platform | OS | Notes |
|---|---|---|
| Jetson Orin (Tegra) | Ubuntu (L4T) | Primary target; GPU and display controller are separate DRM nodes |
| Raspberry Pi 4 / 5 | Raspberry Pi OS | Full KMS overlay required; see below |
| x86 / desktop NVIDIA | Ubuntu | Extra kernel parameter required; see below |

---

## Common setup (all platforms)

### 1. Disable the display manager

The display manager must not be running — it holds the display and
`VK_KHR_display` will fail to acquire it.

```bash
# Ubuntu / L4T
sudo systemctl disable --now gdm

# Raspberry Pi OS
sudo systemctl disable --now lightdm
# or via raspi-config → System Options → Boot → Console (no desktop)
```

### 2. Add the service user to the right groups

The process needs access to `/dev/input/event*` (keyboard via libinput) and
`/dev/dri/*` (Vulkan / DRM).

```bash
# Ubuntu / L4T
sudo usermod -aG input,video $USER

# Raspberry Pi OS — 'render' is used instead of 'video' for GPU nodes
sudo usermod -aG input,video,render $USER
```

If running as a dedicated system user (rather than a login user), set
`SupplementaryGroups=input video` in the unit file (already done).

### 3. Install the service unit

```bash
sudo cp dist/systemd/wonderlamp.service /usr/lib/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wonderlamp
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
No other display manager interaction is needed once this parameter is set.

---

## Packaging (future deb)

The intended package layout when a `.deb` is produced:

| Source path | Installed path |
|---|---|
| `target/release/wonderlamp_server` | `/usr/bin/wonderlamp_server` |
| `dist/systemd/wonderlamp.service` | `/usr/lib/systemd/system/wonderlamp.service` |

With `debhelper` >= 13, placing the unit file under `debian/` and using the
`dh_installsystemd` helper (called automatically by `dh`) handles installation,
`daemon-reload`, `enable`, and `start` on package install/upgrade/remove.

The `debian/wonderlamp.service` symlink (or copy) pointing at
`dist/systemd/wonderlamp.service` keeps a single source of truth.

A `debian/postinst` snippet to disable GDM on install is probably appropriate
for the Jetson target but should prompt the user rather than doing it silently.
