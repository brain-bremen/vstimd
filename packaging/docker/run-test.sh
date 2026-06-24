#!/bin/bash
# Run the vstimd systemd integration test in a Docker container.
#
# Usage: packaging/docker/run-test.sh
#
# Requires:
#   - packaging/vstimd_*.deb (built via Dockerfile.deb-builder)
#   - Docker with privileged cgroup access

set -euo pipefail

IMAGE=vstimd-test-deb

echo "==> Starting systemd container..."
CID=$(docker run -d --privileged \
    --tmpfs /run --tmpfs /run/lock \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
    --cgroupns=host \
    "$IMAGE")
echo "    container: $CID"

cleanup() {
    echo "==> Stopping container..."
    docker stop "$CID" > /dev/null 2>&1 || true
    docker rm   "$CID" > /dev/null 2>&1 || true
}
trap cleanup EXIT

# Wait for systemd to reach running/degraded state (up to 30 s).
echo "==> Waiting for systemd..."
for i in $(seq 1 30); do
    STATE=$(docker exec "$CID" systemctl is-system-running 2>/dev/null || true)
    if echo "$STATE" | grep -qE "^(running|degraded)$"; then
        echo "    systemd ready ($STATE)"
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: systemd did not reach running state within 30 s (state: $STATE)"
        docker exec "$CID" systemctl --failed --no-pager || true
        exit 1
    fi
    sleep 1
done

# Run the test script inside the container.
echo ""
docker exec "$CID" /usr/local/bin/test-service.sh
