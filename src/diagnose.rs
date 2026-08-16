use crate::trace::Trace;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosisKind {
    Healthy,
    TransportStalled,
    DriverStalled,
    ConsumerStalled,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub kind: DiagnosisKind,
    pub irq_delta: Option<u64>,
    pub event_byte_delta: u64,
    pub explanation: String,
}

impl fmt::Display for DiagnosisKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Healthy => "healthy",
            Self::TransportStalled => "transport-stalled",
            Self::DriverStalled => "driver-stalled",
            Self::ConsumerStalled => "consumer-stalled",
            Self::Inconclusive => "inconclusive",
        };
        formatter.write_str(name)
    }
}

pub fn analyze_trace(trace: &Trace) -> Diagnosis {
    let Some(first) = trace.samples.first() else {
        return inconclusive("trace contains no samples");
    };
    let Some(last) = trace.samples.last() else {
        return inconclusive("trace contains no samples");
    };
    let irq_delta = first
        .irq_total
        .zip(last.irq_total)
        .map(|(start, end)| end.saturating_sub(start));
    let event_start: u64 = first.event_bytes.values().copied().sum();
    let event_end: u64 = last.event_bytes.values().copied().sum();
    let event_byte_delta = event_end.saturating_sub(event_start);

    if !trace.expect_motion {
        return Diagnosis {
            kind: DiagnosisKind::Inconclusive,
            irq_delta,
            event_byte_delta,
            explanation: "no motion was requested, so silence may be normal idle behavior".into(),
        };
    }

    let kind = match (irq_delta, event_byte_delta, trace.cursor_stalled) {
        (_, bytes, true) if bytes > 0 => DiagnosisKind::ConsumerStalled,
        (_, bytes, false) if bytes > 0 => DiagnosisKind::Healthy,
        (Some(irqs), 0, _) if irqs > 0 => DiagnosisKind::DriverStalled,
        (Some(0), 0, _) => DiagnosisKind::TransportStalled,
        (None, 0, _) => DiagnosisKind::Inconclusive,
        _ => DiagnosisKind::Inconclusive,
    };
    let explanation = match kind {
        DiagnosisKind::Healthy => "IRQ and evdev activity reached userspace".into(),
        DiagnosisKind::TransportStalled => {
            "expected movement produced neither an Elan IRQ nor evdev bytes".into()
        }
        DiagnosisKind::DriverStalled => {
            "Elan IRQ activity increased, but the driver emitted no evdev bytes".into()
        }
        DiagnosisKind::ConsumerStalled => {
            "evdev bytes were available while the graphical cursor remained stalled".into()
        }
        DiagnosisKind::Inconclusive => "the trace lacks a usable Elan IRQ counter".into(),
    };
    Diagnosis {
        kind,
        irq_delta,
        event_byte_delta,
        explanation,
    }
}

fn inconclusive(explanation: &str) -> Diagnosis {
    Diagnosis {
        kind: DiagnosisKind::Inconclusive,
        irq_delta: None,
        event_byte_delta: 0,
        explanation: explanation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{analyze_trace, DiagnosisKind};
    use crate::trace::{Trace, TraceSample, TRACE_SCHEMA};
    use std::collections::BTreeMap;

    fn fixture(irq_start: Option<u64>, irq_end: Option<u64>, bytes: u64) -> Trace {
        Trace {
            schema: TRACE_SCHEMA.into(),
            captured_unix_ms: 0,
            expect_motion: true,
            cursor_stalled: true,
            controllers: Vec::new(),
            samples: vec![
                TraceSample {
                    elapsed_ms: 0,
                    irq_total: irq_start,
                    event_bytes: BTreeMap::new(),
                },
                TraceSample {
                    elapsed_ms: 1000,
                    irq_total: irq_end,
                    event_bytes: [("/dev/input/event1".into(), bytes)].into(),
                },
            ],
        }
    }

    #[test]
    fn separates_transport_driver_and_consumer_faults() {
        assert_eq!(
            analyze_trace(&fixture(Some(10), Some(10), 0)).kind,
            DiagnosisKind::TransportStalled
        );
        assert_eq!(
            analyze_trace(&fixture(Some(10), Some(12), 0)).kind,
            DiagnosisKind::DriverStalled
        );
        assert_eq!(
            analyze_trace(&fixture(Some(10), Some(12), 24)).kind,
            DiagnosisKind::ConsumerStalled
        );
    }
}
