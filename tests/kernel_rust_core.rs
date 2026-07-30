#[allow(dead_code)]
mod kernel_core {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/rust-shim/elan_core.rs"
    ));
}

use kernel_core::*;

#[test]
fn c_abi_sizes_are_stable() {
    assert_eq!(std::mem::size_of::<ElanRsContact>(), 28);
    assert_eq!(std::mem::size_of::<ElanRsFrame>(), 168);
}

fn decode(report: &[u8], max_x: u32, max_y: u32) -> Result<ElanRsFrame, i32> {
    let mut frame = ElanRsFrame::EMPTY;
    let status = unsafe {
        elan_rs_decode_report(
            report.as_ptr(),
            report.len(),
            max_x,
            max_y,
            100,
            100,
            25,
            1,
            &mut frame,
        )
    };
    if status == 0 {
        Ok(frame)
    } else {
        Err(status)
    }
}

#[test]
fn decodes_normal_touch_contact() {
    let mut report = [0_u8; 32];
    report[2] = 0x5d;
    report[3] = 0x09;
    report[4] = 0x12;
    report[5] = 0x34;
    report[6] = 0x56;
    report[7] = 0x43;
    report[8] = 100;

    let frame = decode(&report, 1024, 1024).expect("valid frame");
    assert_eq!(frame.kind, FRAME_ABSOLUTE);
    assert_eq!(frame.buttons, 1);
    assert_eq!(frame.contact_mask, 1);
    assert_eq!(frame.contacts[0].x, 0x134);
    assert_eq!(frame.contacts[0].y, 1024 - 0x256);
    assert_eq!(frame.contacts[0].pressure, 125);
    assert_eq!(frame.contacts[0].tool_width, 3);
}

#[test]
fn decodes_trackpoint_motion_and_buttons() {
    let report = [0, 0, 0x5e, 0x05, 0x81, 0x7f, 0x06, 10, 20];
    let frame = decode(&report, 1024, 1024).expect("valid frame");
    assert_eq!(frame.kind, FRAME_TRACKPOINT);
    assert_eq!(frame.buttons, 0x05);
    assert_eq!(frame.rel_valid, 1);
    assert_eq!(frame.rel_x, 8);
    assert_eq!(frame.rel_y, 490);
}

#[test]
fn rejects_short_and_unknown_reports() {
    assert_eq!(decode(&[0, 0], 100, 100), Err(-22));
    assert_eq!(decode(&[0, 0, 0xaa], 100, 100), Err(-71));
}

#[test]
fn watchdog_requires_an_open_device_and_real_failure() {
    assert_eq!(elan_rs_watchdog_action(0, -5, 9, 3), WATCHDOG_DISARMED);
    assert_eq!(elan_rs_watchdog_action(1, 0, 2, 3), WATCHDOG_OBSERVE);
    assert_eq!(elan_rs_watchdog_action(1, -5, 0, 3), WATCHDOG_RECOVER);
    assert_eq!(elan_rs_watchdog_action(2, 0, 3, 3), WATCHDOG_RECOVER);
}

#[test]
fn recovery_sequence_is_bounded_and_fail_closed() {
    let mut phase = RECOVERY_IDLE;
    for expected in [
        RECOVERY_RELEASE_INPUT,
        RECOVERY_REINITIALIZE,
        RECOVERY_VERIFY,
        RECOVERY_RESTORE_MODE,
        RECOVERY_COMPLETE,
        RECOVERY_IDLE,
    ] {
        phase = elan_rs_recovery_next(phase, 0);
        assert_eq!(phase, expected);
    }
    assert_eq!(
        elan_rs_recovery_next(RECOVERY_REINITIALIZE, -5),
        RECOVERY_FAILED
    );
}
