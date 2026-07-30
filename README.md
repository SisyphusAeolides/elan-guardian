# Elan Guardian

Elan Guardian is an evidence-driven diagnostic and recovery stack for
Elantech touchpad and TrackPoint controllers attached through Linux I2C/SMBus.
It determines which layer stopped before changing device state:

```text
Elan hardware → IRQ / SMBus → elan_i2c → evdev → input consumer
```

The project does not replace the system SMBus controller, grab physical input
devices, synthesize a virtual mouse, or run a resident daemon.

## Components

- Rust owns the executable diagnosis, watchdog policy, trace replay, and
  narrowly validated recovery state machine.
- Agda proves IRQ, watchdog, and recovery-state invariants.
- Idris 2 provides a total reference policy for fault classification and
  automatic recovery selection.
- Fortran independently scores exported trace and watchdog features for
  differential tests.
- `kernel/` contains the current-kernel C bridge: an upstream-oriented Linux
  patch that runs the same health policy inside `elan_i2c`, automatically
  reinitializing a failed controller without replacing its input objects or
  invalidating userspace file descriptors.

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
patch. On an unpatched kernel it falls back to a bounded unbind/rebind and waits
for evdev nodes to return.

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

## Build and verify

```bash
make all
make check
```

`make check` runs Rust tests and lints, the independent Fortran classifier,
Agda safe-mode proofs, and Idris totality checks when those compilers are
installed.

## Packaging

The RPM installs the Rust and Fortran tools, manual page, and a non-resident
systemd sleep unit for kernels that do not yet contain the watchdog patch. The
unit runs recovery after resume only when DMI identifies an affected ThinkPad
P53. Formal sources and the kernel patch are installed as documentation and
remain independently buildable.

Supported build targets:

- Fedora 44
- Fedora Rawhide
- EPEL 9 and EPEL 10
- RHEL 9 and RHEL 10

## Kernel integration

Current RHEL kernels do not enable Rust kernel modules, and neither Fortran,
Idris, nor Agda is suitable for Linux IRQ context. The deployable kernel bridge
is therefore deliberately small C code suitable for review and backport. The
executable reference implementation remains Rust, and its watchdog and fault
policy are independently checked against the Agda, Idris, and Fortran models.

## License

GPL-2.0-only.
