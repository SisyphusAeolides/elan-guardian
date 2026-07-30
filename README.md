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

- Rust records IRQ and evdev activity, classifies failures, replays traces, and
  performs narrowly validated recovery.
- Agda proves IRQ and recovery-state invariants.
- Idris 2 provides a total reference policy for fault classification.
- Fortran independently scores exported trace features for differential tests.
- `kernel/` contains a small upstream-oriented Linux patch that adds in-place
  Elan reinitialization while retaining existing input objects and file
  descriptors.

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
systemd sleep unit. The unit runs recovery after resume only when DMI identifies
an affected ThinkPad P53. Formal sources and the kernel patch are installed as
documentation and remain independently buildable.

Supported build targets:

- Fedora 44
- Fedora Rawhide
- EPEL 9 and EPEL 10
- RHEL 9 and RHEL 10

## Kernel integration

Current RHEL kernels do not enable Rust kernel modules, so the deployable kernel
change is deliberately small C code suitable for review and backport. The
userspace implementation remains Rust, and the state and classification policy
are checked against the Agda, Idris, and Fortran models.

## License

GPL-2.0-only.
