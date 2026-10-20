# Software effects CI device provisioning

The effects fixtures require a real software DMA-BUF allocator through
`/dev/udmabuf`; failing this prerequisite fails CI rather than substituting a
renderer or skipping the tests.

Device availability is checked before attempting module loading. A built-in
kernel driver need not have a loadable module, and `modprobe` failure alone does
not establish driver absence. The exact Ubuntu Azure 6.17.0-1022.22 modules
package contains `CONFIG_UDMABUF=y` in its boot configuration but no udmabuf entry
in `modules.builtin`. Its corresponding extra-modules package contains no
udmabuf module. Installing an unrelated kernel or extra package is unnecessary
when the character device already exists.

The provisioning helper retains kernel, device, sysfs and configuration
diagnostics even on failure. It restricts device access to the ephemeral runner
user. It makes no kernel upgrade, reboot or out-of-tree module installation.
The hosted run remains the acceptance proof of actual device availability;
unit tests exercise built-in, missing-device and device-after-load branches
using temporary paths and stub privileged commands, without host changes.

Official package source: https://packages.ubuntu.com/noble-updates/linux-modules-6.17.0-1022-azure
Upstream driver configuration: https://github.com/torvalds/linux/blob/v6.17/drivers/dma-buf/Kconfig
