# Rust-core `elan_i2c` module

This directory builds an ordinary Linux `elan_i2c.ko` without requiring
`CONFIG_RUST`. Linux registration, transport calls, input emission, power
management, firmware handling, and module metadata remain behind a C ABI shim.
The `#![no_std]` Rust core owns packet decoding, watchdog decisions, and the
bounded recovery state machine.

The C transport sources are derived from Linux v6.12 and carry their upstream
GPL-2.0-only notices. The watchdog changes match the sibling kernel patch in
this repository.

Build for the running kernel:

```text
make -C kernel/rust-shim
modinfo kernel/rust-shim/elan_i2c.ko
```

Requirements are a C kernel-module toolchain, matching kernel-devel headers,
and stable `rustc`. The build invokes `rustc` directly with kernel code model,
static relocation, no red zone, no unwinding, and no standard library. It does
not use or link the kernel Rust support crate.

The resulting module intentionally has the same name and device aliases as the
stock driver. Do not load it beside the stock `elan_i2c` module. Installation
must preserve a known-good module and initramfs so the change remains
recoverable.
