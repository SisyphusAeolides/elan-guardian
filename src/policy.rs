use serde::{Deserialize, Serialize};

pub const REPORT_ERROR_LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WatchdogAction {
    Disarmed,
    Observe,
    RecoverInPlace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchdogEvidence {
    pub input_open: bool,
    pub transport_probe_ok: bool,
    pub consecutive_report_errors: u32,
}

pub fn watchdog_action(evidence: WatchdogEvidence) -> WatchdogAction {
    if !evidence.input_open {
        WatchdogAction::Disarmed
    } else if !evidence.transport_probe_ok
        || evidence.consecutive_report_errors >= REPORT_ERROR_LIMIT
    {
        WatchdogAction::RecoverInPlace
    } else {
        WatchdogAction::Observe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_idle_controller_is_observed_without_reset() {
        assert_eq!(
            watchdog_action(WatchdogEvidence {
                input_open: true,
                transport_probe_ok: true,
                consecutive_report_errors: 0,
            }),
            WatchdogAction::Observe
        );
    }

    #[test]
    fn failed_transport_probe_requests_in_place_recovery() {
        assert_eq!(
            watchdog_action(WatchdogEvidence {
                input_open: true,
                transport_probe_ok: false,
                consecutive_report_errors: 0,
            }),
            WatchdogAction::RecoverInPlace
        );
    }

    #[test]
    fn repeated_report_failures_request_in_place_recovery() {
        assert_eq!(
            watchdog_action(WatchdogEvidence {
                input_open: true,
                transport_probe_ok: true,
                consecutive_report_errors: REPORT_ERROR_LIMIT,
            }),
            WatchdogAction::RecoverInPlace
        );
    }

    #[test]
    fn closed_input_never_triggers_recovery() {
        assert_eq!(
            watchdog_action(WatchdogEvidence {
                input_open: false,
                transport_probe_ok: false,
                consecutive_report_errors: REPORT_ERROR_LIMIT,
            }),
            WatchdogAction::Disarmed
        );
    }
}
