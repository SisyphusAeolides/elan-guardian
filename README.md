# Elan Guardian

Elan Guardian is an evidence-driven diagnostic and recovery stack for
Elantech touchpad and TrackPoint controllers attached through Linux I2C/SMBus.
It determines which layer stopped before changing device state:

```text
Elan hardware → IRQ / SMBus → elan_i2c → evdev → input consumer
```

The project does not replace the system SMBus controller, grab physical input
devices, or synthesize a virtual mouse. Its resident recovery monitor reads the
kernel watchdog counter and polls existing libinput descriptors without
opening or consuming evdev events.

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

## Install on DNF/RPM based systems

```bash
sudo dnf copr enable sisyphuscode/elan-guardian
sudo dnf install elan-guardian
sudo systemctl enable --now elan-guardian-resume.service
```
## Install on Arch

Add the Sisyphus repository to `/etc/pacman.conf`:

```ini
[sisyphus]
SigLevel = Optional TrustAll
Server = https://sisyphusaeolides.github.io/Sisyphus-Repo/$arch
```

Then install the userspace tools:

```bash
sudo pacman -Syy
sudo pacman -S elan-guardian
```

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

The kernel patch includes a non-resident workqueue watchdog. It is enabled
by default on the Lenovo ThinkPad P53 and can be enabled for another validated
machine through the `elan,runtime-watchdog` firmware property. The watchdog:

- is armed only while at least one Elan input node is open;
- requests immediate recovery after three consecutive report-read errors;
- performs no periodic controller I/O; and
- leaves healthy active and idle controllers untouched.

Its state, successful report count, current error streak, and automatic-
recovery count are exported through the read-only `runtime_watchdog` sysfs
attribute and shown by
`elan-guardian status`. The manual `recover` attribute remains available as a
bounded fallback.

On affected ThinkPad P53 systems, the optional
`elan-guardian-module.service` first compares the running and installed module
identities and safely activates the installed external module when they differ.
A failed module build never disables the portable
userspace resume and consumer recovery paths. Then
`elan-guardian-watch.service` watches that counter and finds ELAN descriptors
already registered in a libinput consumer's
epoll set. It duplicates those existing file descriptions with `pidfd_getfd`
only to poll readiness; it never reads, opens, or grabs an evdev node. If a
descriptor remains continuously readable for 750 ms, the consumer has stopped
draining a live kernel stream and the monitor rebinds the controller. A
five-second cooldown prevents recovery loops while the desktop consumes the
replacement Touchpad and TrackPoint hotplug events.

When `libinput-rs` supplies its P53-specific resume recovery unit, the guardian
defers its own equivalent one-shot resume action so the controller is rebound
exactly once. Live guardian monitoring remains enabled.

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
sudo pacman -S --needed linux-headers binutils gcc make rust
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
  "/usr/lib/modules/$release/updates/elan-guardian/elan_i2c.ko"
sudo depmod "$release"
sudo mkinitcpio -P
modprobe -D elan_i2c
```

Reboot to activate the staged module. The stock module remains in its original
location as the rollback copy. Rebuild for a new kernel whenever kABI checking
does not create a compatible weak-update link.

## Arch packaging

The Arch package installs the Rust and Fortran tools, manual page, an optional
module activation unit, a sleep recovery unit, and the
non-grabbing watchdog monitor. Both recovery units act only when DMI identifies
an affected ThinkPad P53. Formal sources and the in-tree kernel patch remain
independently buildable.

The external module is compiled in CI against Linux 6.12 and Linux 7.1 and can
be rebuilt for each installed kernel. Kernel APIs are not stable, so an
unbuildable future kernel falls back to the distribution's `elan_i2c` module
without failing the package transaction; userspace recovery remains active.

The supported packaging target is x86_64 Arch Linux and compatible Arch-based
distributions.

## Kernel integration

Neither Fortran, Idris, nor Agda is suitable for Linux IRQ context. The hybrid
module therefore
links a freestanding no_std Rust object into an ordinary C-registered kernel
module. C owns only the Linux I2C/SMBus, input, IRQ, firmware, power-management,
and module ABI boundary; Rust decodes reports and controls watchdog/recovery
policy. The same policy remains independently checked against the Agda, Idris,
and Fortran models.

## License

GPL-2.0-only.
