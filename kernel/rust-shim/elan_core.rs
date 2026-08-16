// SPDX-License-Identifier: GPL-2.0-only

use core::ptr;

pub const ABI_VERSION: u32 = 1;
pub const MAX_CONTACTS: usize = 5;

const REPORT_ID_OFFSET: usize = 2;
const TOUCH_INFO_OFFSET: usize = 3;
const FINGER_DATA_OFFSET: usize = 4;
const HOVER_INFO_OFFSET: usize = 30;
const MK_DATA_OFFSET: usize = 33;
const FINGER_DATA_LEN: usize = 5;

const REPORT_ID: u8 = 0x5d;
const TRACKPOINT_REPORT_ID: u8 = 0x5e;
const TRACKPOINT_REPORT_ID2: u8 = 0x5f;
const REPORT_ID2: u8 = 0x60;

const FEATURE_REPORT_MK: u32 = 1;
const MAX_PRESSURE: i32 = 255;
const FINGER_WIDTH_REDUCE: u32 = 90;

pub const FRAME_ABSOLUTE: u32 = 1;
pub const FRAME_TRACKPOINT: u32 = 2;

pub const WATCHDOG_DISARMED: u32 = 0;
pub const WATCHDOG_OBSERVE: u32 = 1;
pub const WATCHDOG_RECOVER: u32 = 2;

pub const RECOVERY_IDLE: u32 = 0;
pub const RECOVERY_RELEASE_INPUT: u32 = 1;
pub const RECOVERY_REINITIALIZE: u32 = 2;
pub const RECOVERY_VERIFY: u32 = 3;
pub const RECOVERY_RESTORE_MODE: u32 = 4;
pub const RECOVERY_COMPLETE: u32 = 5;
pub const RECOVERY_FAILED: u32 = 6;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElanRsContact {
    pub active: u32,
    pub x: u32,
    pub y: u32,
    pub pressure: u32,
    pub tool_width: u32,
    pub touch_major: u32,
    pub touch_minor: u32,
}

impl ElanRsContact {
    const EMPTY: Self = Self {
        active: 0,
        x: 0,
        y: 0,
        pressure: 0,
        tool_width: 0,
        touch_major: 0,
        touch_minor: 0,
    };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElanRsFrame {
    pub kind: u32,
    pub buttons: u32,
    pub hover: u32,
    pub contact_mask: u32,
    pub contacts: [ElanRsContact; MAX_CONTACTS],
    pub rel_x: i32,
    pub rel_y: i32,
    pub rel_valid: u32,
}

impl ElanRsFrame {
    pub const EMPTY: Self = Self {
        kind: 0,
        buttons: 0,
        hover: 0,
        contact_mask: 0,
        contacts: [ElanRsContact::EMPTY; MAX_CONTACTS],
        rel_x: 0,
        rel_y: 0,
        rel_valid: 0,
    };
}

#[inline]
unsafe fn read_byte(report: *const u8, report_len: usize, offset: usize) -> Option<u8> {
    if offset >= report_len {
        None
    } else {
        // SAFETY: The caller supplies a report buffer valid for report_len bytes,
        // and the bounds check above proves offset is in that allocation.
        Some(unsafe { ptr::read(report.add(offset)) })
    }
}

#[inline]
fn scaled_pressure(raw: u8, adjustment: i32) -> u32 {
    (i32::from(raw) + adjustment).clamp(0, MAX_PRESSURE) as u32
}

#[allow(clippy::too_many_arguments)]
unsafe fn decode_absolute(
    report: *const u8,
    report_len: usize,
    high_precision: bool,
    max_x: u32,
    max_y: u32,
    width_x: u32,
    width_y: u32,
    pressure_adjustment: i32,
    report_features: u32,
    frame: &mut ElanRsFrame,
) -> i32 {
    let Some(tp_info) = (unsafe { read_byte(report, report_len, TOUCH_INFO_OFFSET) }) else {
        return -22;
    };

    frame.kind = FRAME_ABSOLUTE;
    frame.buttons = u32::from(tp_info & 0x07);
    frame.hover = unsafe { read_byte(report, report_len, HOVER_INFO_OFFSET) }
        .map_or(0, |v| u32::from((v & 0x40) != 0));

    let mut finger_offset = FINGER_DATA_OFFSET;
    let area_x_unit = width_x.saturating_sub(FINGER_WIDTH_REDUCE);
    let area_y_unit = width_y.saturating_sub(FINGER_WIDTH_REDUCE);

    for slot in 0..MAX_CONTACTS {
        if tp_info & (1 << (3 + slot)) == 0 {
            continue;
        }

        let end = match finger_offset.checked_add(FINGER_DATA_LEN) {
            Some(end) => end,
            None => return -22,
        };
        if end > report_len {
            return -22;
        }

        let b0 = unsafe { read_byte(report, report_len, finger_offset) }.unwrap_or(0);
        let b1 = unsafe { read_byte(report, report_len, finger_offset + 1) }.unwrap_or(0);
        let b2 = unsafe { read_byte(report, report_len, finger_offset + 2) }.unwrap_or(0);
        let b3 = unsafe { read_byte(report, report_len, finger_offset + 3) }.unwrap_or(0);
        let b4 = unsafe { read_byte(report, report_len, finger_offset + 4) }.unwrap_or(0);

        let (x, y) = if high_precision {
            (
                (u32::from(b0) << 8) | u32::from(b1),
                (u32::from(b2) << 8) | u32::from(b3),
            )
        } else {
            (
                (u32::from(b0 & 0xf0) << 4) | u32::from(b1),
                (u32::from(b0 & 0x0f) << 8) | u32::from(b2),
            )
        };

        if x <= max_x && y <= max_y {
            let contact = &mut frame.contacts[slot];
            contact.active = 1;
            contact.x = x;
            contact.y = max_y - y;
            contact.pressure = scaled_pressure(b4, pressure_adjustment);
            frame.contact_mask |= 1 << slot;

            if report_features & FEATURE_REPORT_MK != 0 {
                let mk = if high_precision {
                    match unsafe { read_byte(report, report_len, MK_DATA_OFFSET + slot) } {
                        Some(value) => value,
                        None => return -22,
                    }
                } else {
                    b3
                };
                let mk_x = u32::from(mk & 0x0f);
                let mk_y = u32::from(mk >> 4);
                let area_x = mk_x.saturating_mul(area_x_unit);
                let area_y = mk_y.saturating_mul(area_y_unit);
                contact.tool_width = mk_x;
                contact.touch_major = area_x.max(area_y);
                contact.touch_minor = area_x.min(area_y);
            }
        }

        finger_offset = end;
    }

    0
}

unsafe fn decode_trackpoint(
    report: *const u8,
    report_len: usize,
    frame: &mut ElanRsFrame,
) -> i32 {
    let packet = REPORT_ID_OFFSET + 1;
    if packet + 6 > report_len {
        return -22;
    }

    let b0 = unsafe { read_byte(report, report_len, packet) }.unwrap_or(0);
    let b1 = unsafe { read_byte(report, report_len, packet + 1) }.unwrap_or(0);
    let b2 = unsafe { read_byte(report, report_len, packet + 2) }.unwrap_or(0);
    let b3 = unsafe { read_byte(report, report_len, packet + 3) }.unwrap_or(0);
    let b4 = unsafe { read_byte(report, report_len, packet + 4) }.unwrap_or(0);
    let b5 = unsafe { read_byte(report, report_len, packet + 5) }.unwrap_or(0);

    frame.kind = FRAME_TRACKPOINT;
    frame.buttons = u32::from(b0 & 0x07);
    if b3 & 0x0f == 0x06 {
        frame.rel_x = i32::from(b4) - (i32::from(b1 ^ 0x80) << 1);
        frame.rel_y = (i32::from(b2 ^ 0x80) << 1) - i32::from(b5);
        frame.rel_valid = 1;
    }

    0
}

/// Decodes one Elan transport report into a C-layout frame.
///
/// Returns zero on success and a negative Linux errno on malformed input.
///
/// # Safety
///
/// `report` must reference `report_len` readable bytes and `out` must reference
/// one writable `ElanRsFrame`. The two allocations must not overlap.
#[no_mangle]
pub unsafe extern "C" fn elan_rs_decode_report(
    report: *const u8,
    report_len: usize,
    max_x: u32,
    max_y: u32,
    width_x: u32,
    width_y: u32,
    pressure_adjustment: i32,
    report_features: u32,
    out: *mut ElanRsFrame,
) -> i32 {
    if report.is_null() || out.is_null() {
        return -22;
    }

    let Some(report_id) = (unsafe { read_byte(report, report_len, REPORT_ID_OFFSET) }) else {
        return -22;
    };

    let words = out.cast::<u32>();
    let word_count = core::mem::size_of::<ElanRsFrame>() / core::mem::size_of::<u32>();
    for index in 0..word_count {
        // SAFETY: ElanRsFrame contains only 32-bit fields, out points to one
        // writable frame, and index stays inside that frame.
        unsafe { ptr::write_volatile(words.add(index), 0) };
    }
    // SAFETY: out is non-null and the caller guarantees writable, correctly
    // aligned storage for one frame.
    let frame = unsafe { &mut *out };

    match report_id {
        REPORT_ID => unsafe {
            decode_absolute(
                report,
                report_len,
                false,
                max_x,
                max_y,
                width_x,
                width_y,
                pressure_adjustment,
                report_features,
                frame,
            )
        },
        REPORT_ID2 => unsafe {
            decode_absolute(
                report,
                report_len,
                true,
                max_x,
                max_y,
                width_x,
                width_y,
                pressure_adjustment,
                report_features,
                frame,
            )
        },
        TRACKPOINT_REPORT_ID | TRACKPOINT_REPORT_ID2 => unsafe {
            decode_trackpoint(report, report_len, frame)
        },
        _ => -71,
    }
}

/// Pure watchdog policy shared by IRQ-error and periodic-probe paths.
#[no_mangle]
pub extern "C" fn elan_rs_watchdog_action(
    open_count: u32,
    probe_status: i32,
    report_errors: u32,
    report_error_threshold: u32,
) -> u32 {
    if open_count == 0 {
        WATCHDOG_DISARMED
    } else if probe_status < 0
        || (report_error_threshold != 0 && report_errors >= report_error_threshold)
    {
        WATCHDOG_RECOVER
    } else {
        WATCHDOG_OBSERVE
    }
}

/// Advances the bounded in-place recovery sequence after one shim operation.
#[no_mangle]
pub extern "C" fn elan_rs_recovery_next(phase: u32, operation_status: i32) -> u32 {
    if operation_status < 0 {
        return RECOVERY_FAILED;
    }

    match phase {
        RECOVERY_IDLE => RECOVERY_RELEASE_INPUT,
        RECOVERY_RELEASE_INPUT => RECOVERY_REINITIALIZE,
        RECOVERY_REINITIALIZE => RECOVERY_VERIFY,
        RECOVERY_VERIFY => RECOVERY_RESTORE_MODE,
        RECOVERY_RESTORE_MODE => RECOVERY_COMPLETE,
        RECOVERY_COMPLETE => RECOVERY_IDLE,
        _ => RECOVERY_FAILED,
    }
}

#[no_mangle]
pub extern "C" fn elan_rs_abi_version() -> u32 {
    ABI_VERSION
}
