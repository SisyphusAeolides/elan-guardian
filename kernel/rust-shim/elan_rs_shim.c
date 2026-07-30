// SPDX-License-Identifier: GPL-2.0-only

#include <linux/build_bug.h>
#include <linux/errno.h>
#include <linux/input/mt.h>
#include <linux/pm_wakeup.h>

#include "elan_rs_shim.h"

static_assert(sizeof(struct elan_rs_contact) == 28);
static_assert(sizeof(struct elan_rs_frame) == 168);

static void elan_rs_emit_absolute(const struct elan_rs_report_context *context,
				  const struct elan_rs_frame *frame)
{
	struct input_dev *input = context->touchpad;
	int slot;

	pm_wakeup_event(context->dev, 0);

	for (slot = 0; slot < ELAN_RS_MAX_CONTACTS; slot++) {
		const struct elan_rs_contact *contact = &frame->contacts[slot];

		input_mt_slot(input, slot);
		if (!contact->active) {
			input_mt_report_slot_inactive(input);
			continue;
		}

		input_mt_report_slot_state(input, MT_TOOL_FINGER, true);
		input_report_abs(input, ABS_MT_POSITION_X, contact->x);
		input_report_abs(input, ABS_MT_POSITION_Y, contact->y);
		input_report_abs(input, ABS_MT_PRESSURE, contact->pressure);
		if (context->report_features) {
			input_report_abs(input, ABS_TOOL_WIDTH,
					 contact->tool_width);
			input_report_abs(input, ABS_MT_TOUCH_MAJOR,
					 contact->touch_major);
			input_report_abs(input, ABS_MT_TOUCH_MINOR,
					 contact->touch_minor);
		}
	}

	input_report_key(input, BTN_LEFT, frame->buttons & BIT(0));
	input_report_key(input, BTN_MIDDLE, frame->buttons & BIT(2));
	input_report_key(input, BTN_RIGHT, frame->buttons & BIT(1));
	input_report_abs(input, ABS_DISTANCE, frame->hover != 0);
	input_mt_report_pointer_emulation(input, true);
	input_sync(input);
}

static int elan_rs_emit_trackpoint(const struct elan_rs_report_context *context,
				   const struct elan_rs_frame *frame)
{
	struct input_dev *input = context->trackpoint;

	if (!input)
		return -ENODEV;

	pm_wakeup_event(context->dev, 0);
	input_report_key(input, BTN_LEFT, frame->buttons & BIT(0));
	input_report_key(input, BTN_RIGHT, frame->buttons & BIT(1));
	input_report_key(input, BTN_MIDDLE, frame->buttons & BIT(2));
	if (frame->rel_valid) {
		input_report_rel(input, REL_X, frame->rel_x);
		input_report_rel(input, REL_Y, frame->rel_y);
	}
	input_sync(input);

	return 0;
}

int elan_rs_emit_report(const struct elan_rs_report_context *context,
			const u8 *report, size_t report_len)
{
	struct elan_rs_frame frame;
	int error;

	error = elan_rs_decode_report(report, report_len,
				      context->max_x, context->max_y,
				      context->width_x, context->width_y,
				      context->pressure_adjustment,
				      context->report_features, &frame);
	if (error)
		return error;

	switch (frame.kind) {
	case ELAN_RS_FRAME_ABSOLUTE:
		elan_rs_emit_absolute(context, &frame);
		return 0;
	case ELAN_RS_FRAME_TRACKPOINT:
		return elan_rs_emit_trackpoint(context, &frame);
	default:
		return -EPROTO;
	}
}
