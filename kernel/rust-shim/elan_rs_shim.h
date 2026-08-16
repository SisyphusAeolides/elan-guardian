/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef _ELAN_RS_SHIM_H
#define _ELAN_RS_SHIM_H

#include <linux/device.h>
#include <linux/input.h>
#include <linux/types.h>

#define ELAN_RS_ABI_VERSION 1
#define ELAN_RS_MAX_CONTACTS 5

#define ELAN_RS_FRAME_ABSOLUTE 1
#define ELAN_RS_FRAME_TRACKPOINT 2

#define ELAN_RS_WATCHDOG_DISARMED 0
#define ELAN_RS_WATCHDOG_OBSERVE 1
#define ELAN_RS_WATCHDOG_RECOVER 2

#define ELAN_RS_RECOVERY_IDLE 0
#define ELAN_RS_RECOVERY_RELEASE_INPUT 1
#define ELAN_RS_RECOVERY_REINITIALIZE 2
#define ELAN_RS_RECOVERY_VERIFY 3
#define ELAN_RS_RECOVERY_RESTORE_MODE 4
#define ELAN_RS_RECOVERY_COMPLETE 5
#define ELAN_RS_RECOVERY_FAILED 6

struct elan_rs_contact {
	u32 active;
	u32 x;
	u32 y;
	u32 pressure;
	u32 tool_width;
	u32 touch_major;
	u32 touch_minor;
};

struct elan_rs_frame {
	u32 kind;
	u32 buttons;
	u32 hover;
	u32 contact_mask;
	struct elan_rs_contact contacts[ELAN_RS_MAX_CONTACTS];
	s32 rel_x;
	s32 rel_y;
	u32 rel_valid;
};

struct elan_rs_report_context {
	struct device *dev;
	struct input_dev *touchpad;
	struct input_dev *trackpoint;
	u32 max_x;
	u32 max_y;
	u32 width_x;
	u32 width_y;
	s32 pressure_adjustment;
	u32 report_features;
};

u32 elan_rs_abi_version(void);
u32 elan_rs_watchdog_action(u32 open_count, s32 probe_status,
			    u32 report_errors, u32 report_error_threshold);
u32 elan_rs_recovery_next(u32 phase, s32 operation_status);
int elan_rs_decode_report(const u8 *report, size_t report_len,
			  u32 max_x, u32 max_y, u32 width_x, u32 width_y,
			  s32 pressure_adjustment, u32 report_features,
			  struct elan_rs_frame *out);

int elan_rs_emit_report(const struct elan_rs_report_context *context,
			const u8 *report, size_t report_len);

#endif
