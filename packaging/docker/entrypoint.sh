#!/bin/bash
# Start systemd as PID 1. The test is run externally via `docker exec`
# once systemd reports the system is running.
exec /sbin/init
