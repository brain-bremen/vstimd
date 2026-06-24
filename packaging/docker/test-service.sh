#!/bin/bash
# Integration test: install vstimd .deb and exercise the full systemd lifecycle.
#
# Checks:
#   1. dpkg -i succeeds (package installs cleanly)
#   2. vstimd system user created and added to input/video groups
#   3. systemctl start vstimd reaches active(running) within 20 s
#      (uses VSTIMD_NULL=1 — no GPU or display required)
#   4. ZMQ port 5555 is reachable
#   5. systemctl stop vstimd exits cleanly within TimeoutStopSec
#   6. Service is inactive after stop (no zombie / leaked process)
#   7. apt-get purge removes the vstimd user

set -euo pipefail

PASS=0
FAIL=0

pass() { echo "  PASS: $*"; ((PASS++)) || true; }
fail() { echo "  FAIL: $*"; ((FAIL++)) || true; }

echo "=== vstimd systemd integration test ==="
echo ""

# ── 1. Install the package ────────────────────────────────────────────────────
echo "--- 1. apt-get install /pkg/vstimd.deb"
apt-get update -qq
if apt-get install -y /pkg/vstimd.deb 2>&1; then
    pass "package installed"
else
    fail "apt-get install failed"
    exit 1
fi

# ── 2. User and group membership ─────────────────────────────────────────────
echo ""
echo "--- 2. vstimd user and groups"
if id -u vstimd > /dev/null 2>&1; then
    pass "vstimd system user exists"
else
    fail "vstimd user was not created"
fi

for grp in input video; do
    if getent group "$grp" > /dev/null 2>&1; then
        if id -nG vstimd | tr ' ' '\n' | grep -qx "$grp"; then
            pass "vstimd in group $grp"
        else
            fail "vstimd NOT in group $grp"
        fi
    else
        echo "  SKIP: group $grp not present on this system"
    fi
done

# Use null renderer so no GPU or display hardware is required.
mkdir -p /etc/systemd/system/vstimd.service.d
cat > /etc/systemd/system/vstimd.service.d/null-renderer.conf <<'EOF'
[Service]
Environment=VSTIMD_NULL=1
# Remove the TTY directives that require a physical console.
TTYPath=
StandardInput=null
TTYReset=no
TTYVHangup=no
EOF

systemctl daemon-reload

# ── 2. Start and wait for READY ───────────────────────────────────────────────
echo ""
echo "--- 3. systemctl start vstimd (Type=notify — waits for READY=1)"
if timeout 20 systemctl start vstimd; then
    pass "service reached active state"
else
    fail "service failed to start within 20 s"
    journalctl -u vstimd --no-pager -n 40
    exit 1
fi

state=$(systemctl is-active vstimd 2>/dev/null || true)
if [ "$state" = "active" ]; then
    pass "systemctl is-active = active"
else
    fail "systemctl is-active = $state (expected active)"
fi

# ── 3. ZMQ port reachable ─────────────────────────────────────────────────────
echo ""
echo "--- 4. ZMQ port 5555 reachable"
if timeout 5 bash -c 'until nc -z 127.0.0.1 5555; do sleep 0.2; done' 2>/dev/null; then
    pass "port 5555 open"
else
    fail "port 5555 not reachable within 5 s"
fi

# ── 4. Stop cleanly ───────────────────────────────────────────────────────────
echo ""
echo "--- 5. systemctl stop vstimd"
if timeout 10 systemctl stop vstimd; then
    pass "service stopped cleanly"
else
    fail "service did not stop within 10 s"
fi

# ── 5. No zombie after stop ───────────────────────────────────────────────────
echo ""
echo "--- 6. service inactive after stop"
state=$(systemctl is-active vstimd 2>/dev/null || true)
if [ "$state" = "inactive" ]; then
    pass "systemctl is-active = inactive"
else
    fail "systemctl is-active = $state (expected inactive)"
fi

if pgrep -x vstimd > /dev/null; then
    fail "vstimd process still running after stop"
else
    pass "no vstimd process remaining"
fi

# ── 7. Purge removes the system user ─────────────────────────────────────────
echo ""
echo "--- 7. apt-get purge vstimd"
if apt-get purge -y vstimd 2>&1; then
    pass "package purged"
else
    fail "apt-get purge failed"
fi

if id -u vstimd > /dev/null 2>&1; then
    fail "vstimd user still exists after purge"
else
    pass "vstimd user removed by purge"
fi

# ── Result ────────────────────────────────────────────────────────────────────
echo ""
echo "=== result: $PASS passed, $FAIL failed ==="
journalctl -u vstimd --no-pager -n 20

[ "$FAIL" -eq 0 ]
