# Elan Guardian

Elan Guardian is an evidence-driven diagnostic and recovery stack for
Elantech touchpad and TrackPoint controllers attached through Linux I2C/SMBus.
It determines which layer stopped before changing device state:

```text
Elan hardware → IRQ / SMBus → elan_i2c → evdev → input consumer
```

The project does not replace the system SMBus controller, grab physical input
devices, or synthesize a virtual mouse. Its optional resident recovery monitor
reads only the kernel watchdog counter and never opens evdev.

## Components

- Rust owns the executable diagnosis, trace replay, Elan report decoder,
  watchdog policy, and bounded recovery state machine.
- Agda proves IRQ, watchdog, and recovery-state invariants.
- Idris 2 provides a total reference policy for fault classification and
  automatic recovery selection.
- Fortran independently scores exported trace and watchdog features for
  differential tests.
- `kernel/rust-shim/` builds an ordinary `elan_i2c.ko` with a `#![no_std]`
  Rust data/policy core and a C Linux ABI shim. It works on kernels that have
  `CONFIG_RUST` disabled.
- `kernel/` also contains an upstream-oriented Linux patch that embeds the
  same health policy in the in-tree driver.

## Diagnose a stalled cursor

When the cursor is stalled, move the touchpad or TrackPoint continuously while
recording:

```bash
sudo elan-guardian record \
  --duration 10 \
  --expect-motion \
  --cursor-stalled \
  --output elan-stall.trace.json
elan-guardian analyze elan-stall.trace.json
```

Interpretation:

| Result | Evidence |
| --- | --- |
| `transport-stalled` | No Elan IRQ and no evdev bytes during expected motion |
| `driver-stalled` | IRQ count increased but evdev produced no bytes |
| `consumer-stalled` | Evdev produced bytes while the cursor remained stalled |
| `healthy` | IRQ and evdev activity reached userspace |
| `inconclusive` | Motion was not requested or the IRQ counter was unavailable |

Capture evidence before recovery. Then recover dynamically discovered
controllers with:

```bash
sudo elan-guardian recover --all
```

The command prefers the in-place recovery interface supplied by the kernel
patch. If that operation fails, or on an unpatched kernel, it falls back to a
bounded unbind/rebind and waits for evdev nodes to return. Use `--rebind` to
request the hard fallback directly.

## Automatic prevention and recovery

The kernel patch includes a non-resident delayed-work watchdog. It is enabled
by default on the Lenovo ThinkPad P53 and can be enabled for another validated
machine through the `elan,runtime-watchdog` firmware property. The watchdog:

- is armed only while at least one Elan input node is open;
- probes the live transport every five seconds;
- requests immediate recovery after three consecutive report-read errors;
- reinitializes in place when a live probe fails; and
- leaves a healthy but idle controller untouched.

Its state and successful automatic-recovery count are exported through the
read-only `runtime_watchdog` sysfs attribute and shown by
`elan-guardian status`. The manual `recover` attribute remains available as a
bounded fallback.

On affected ThinkPad P53 systems, `elan-guardian-watch.service` watches only
that counter. When the kernel has attempted an in-place recovery, the monitor
rebinds the controller so the desktop receives fresh Touchpad and TrackPoint
devices. This closes the case where the transport reset succeeds but an
existing userspace input path remains unusable.

## Build and verify

```bash
make all
make check
```

`make check` runs Rust tests and lints, the independent Fortran classifier,
Agda safe-mode proofs, and Idris totality checks when those compilers are
installed.

Build the replacement module for the running kernel with:

```bash
sudo dnf install "kernel-devel-$(uname -r)" binutils gcc make rust
make kmod
modinfo kernel/rust-shim/elan_i2c.ko
```

The build compiles the Rust core directly with the kernel code model, no red
zone, no unwinding, return-thunk and IBT settings from the target kernel, and
then rejects SIMD/FPU instructions, unexpected runtime symbols, or unmitigated
indirect branches. The resulting module has the exact target-kernel vermagic
and modversions.

To stage it without overwriting the distribution module:

```bash
release=$(uname -r)
sudo install -Dm644 kernel/rust-shim/elan_i2c.ko \
  "/lib/modules/$release/updates/elan-guardian/elan_i2c.ko"
sudo depmod -a "$release"
printf '%s\n' "/lib/modules/$release/updates/elan-guardian/elan_i2c.ko" | \
  sudo weak-modules --add-modules --no-initramfs
modprobe -D elan_i2c
```

Reboot to activate the staged module. The stock module remains in its original
location as the rollback copy. Rebuild for a new kernel whenever kABI checking
does not create a compatible weak-update link.

## Packaging

The RPM installs the Rust and Fortran tools, manual page, module source under
`/usr/src/elan-guardian-0.2.1/rust-shim`, a sleep recovery unit, and the
non-grabbing watchdog monitor. Both recovery units act only when DMI identifies
an affected ThinkPad P53. Formal sources and the in-tree kernel patch remain
independently buildable.

Supported build targets:

- Fedora 44
- Fedora Rawhide
- EPEL 9 and EPEL 10
- RHEL 9 and RHEL 10

## Kernel integration

Current RHEL kernels do not enable Rust kernel modules, and neither Fortran,
Idris, nor Agda is suitable for Linux IRQ context. The hybrid module therefore
links a freestanding no_std Rust object into an ordinary C-registered kernel
module. C owns only the Linux I2C/SMBus, input, IRQ, firmware, power-management,
and module ABI boundary; Rust decodes reports and controls watchdog/recovery
policy. The same policy remains independently checked against the Agda, Idris,
and Fortran models.

## License

GPL-2.0-only.
