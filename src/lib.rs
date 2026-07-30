//! Evidence-driven diagnostics and recovery for Elantech I2C controllers.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod device;
pub mod diagnose;
pub mod irq;
pub mod recover;
pub mod trace;

pub use device::{affected_thinkpad_p53, discover, Controller};
pub use diagnose::{analyze_trace, Diagnosis, DiagnosisKind};
pub use recover::{recover, RecoverMethod, RecoveredController};
pub use trace::{record, RecordOptions, Trace, TraceSample, TRACE_SCHEMA};
