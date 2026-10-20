#!/usr/bin/env bash
# The driver may be built in without an entry in modules.builtin.
set -euo pipefail
artifacts=${1:?artifact directory required}
mkdir -p "$artifacts"
{
  uname -a
  ls -l /dev/udmabuf /sys/class/misc/udmabuf 2>&1 || true
  grep -E 'CONFIG_(UDMABUF|MEMFD_CREATE|DMA_SHARED_BUFFER)=' "/boot/config-$(uname -r)" 2>/dev/null || true
  grep -i udmabuf "/lib/modules/$(uname -r)/modules.builtin" 2>/dev/null || true
} > "$artifacts/kernel.txt"
if [[ ! -c /dev/udmabuf ]]; then
  sudo modprobe udmabuf > "$artifacts/modprobe.log" 2>&1 || true
  sudo udevadm settle
fi
if [[ ! -c /dev/udmabuf ]]; then
  echo '::error::Software effects require /dev/udmabuf; see kernel diagnostics. No effects acceptance was skipped.' >&2
  exit 1
fi
sudo chown "$(id -u):$(id -g)" /dev/udmabuf
sudo chmod 0600 /dev/udmabuf
