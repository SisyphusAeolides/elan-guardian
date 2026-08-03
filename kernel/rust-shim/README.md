# Rust-core `elan_i2c` module

This directory builds an ordinary Linux `elan_i2c.ko` without requiring
`CONFIG_RUST`. Linux registration, transport calls, input emission, power
management, firmware handling, and module metadata remain behind a C ABI shim.
The `#![no_std]` Rust core owns packet decoding, watchdog decisions, and the
bounded recovery state machine.

The C transport sources use Linux v6.12-compatible syntax, carry the
substantive upstream ELAN fixes through Linux v7.1, and retain their upstream
GPL-2.0-only notices. The watchdog changes match the sibling kernel patch in
this repository. CI compiles the external module against both kernel lines.

Build for the running kernel:

```text
make -C kernel/rust-shim
modinfo kernel/rust-shim/elan_i2c.ko
```

Requirements are a C kernel-module toolchain, matching kernel-devel headers,
and stable `rustc`. The build invokes `rustc` directly with kernel code model,
static relocation, no red zone, no unwinding, and no standard library. It does
not use or link the kernel Rust support crate. For kernels built with Clang,
the Makefile reads `CONFIG_CC_IS_CLANG` and automatically selects Kbuild's LLVM
toolchain mode.

The resulting optional module intentionally has the same name and device
aliases as the stock driver. Do not load it beside the stock `elan_i2c`
module. Installation must preserve a known-good module and initramfs so the change remains
recoverable. If a future kernel cannot build the optional module, the packaged
userspace recovery continues with the distribution driver.

## Current ArachOS integration status

This project is maintained as part of the ArachOS production graph. Its role is
the kernel recovery shim and its explicit driver boundary..

CI and release evidence are evaluated on immutable revisions. Hardware support
is reported by bounded route and support level; this README does not claim
universal native support. Gate 3 requires signed hardware identity, target
kernel provenance, package authority, health checks, rollback behavior, and
representative physical-hardware evidence before production qualification.

## Current ArachOS integration status

This project is maintained as part of the ArachOS production graph. Its role is
the kernel recovery shim and its explicit driver boundary.

CI and release evidence are evaluated on immutable revisions. Hardware support
is reported by bounded route and support level; this README does not claim
universal native support. Gate 3 requires signed hardware identity, target
kernel provenance, package authority, health checks, rollback behavior, and
representative physical-hardware evidence before production qualification.
