// SPDX-License-Identifier: GPL-2.0-only

#![no_std]

include!("elan_core.rs");

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
