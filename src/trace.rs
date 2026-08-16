use crate::device::{discover, Controller};
use crate::irq;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::fd::{FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const TRACE_SCHEMA: &str = "org.elan-guardian.trace.v1";

#[derive(Debug, Clone)]
pub struct RecordOptions {
    pub duration: Duration,
    pub interval: Duration,
    pub expect_motion: bool,
    pub cursor_stalled: bool,
    pub sysfs_root: PathBuf,
    pub proc_interrupts: PathBuf,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(10),
            interval: Duration::from_millis(100),
            expect_motion: false,
            cursor_stalled: false,
            sysfs_root: PathBuf::from("/sys"),
            proc_interrupts: PathBuf::from("/proc/interrupts"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub schema: String,
    pub captured_unix_ms: u128,
    pub expect_motion: bool,
    pub cursor_stalled: bool,
    pub controllers: Vec<Controller>,
    pub samples: Vec<TraceSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSample {
    pub elapsed_ms: u64,
    pub irq_total: Option<u64>,
    pub event_bytes: BTreeMap<PathBuf, u64>,
}

struct EventReader {
    path: PathBuf,
    file: File,
    bytes: u64,
}

pub fn record(options: &RecordOptions) -> io::Result<Trace> {
    if options.duration.is_zero() || options.interval.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "duration and interval must be non-zero",
        ));
    }
    let controllers = discover(&options.sysfs_root)?;
    if controllers.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no controllers are bound to elan_i2c",
        ));
    }
    let mut readers = open_event_nodes(&controllers)?;
    let start = Instant::now();
    let mut samples = Vec::new();

    loop {
        drain_events(&mut readers)?;
        samples.push(TraceSample {
            elapsed_ms: start.elapsed().as_millis() as u64,
            irq_total: irq::total_from_proc(&options.proc_interrupts).ok(),
            event_bytes: readers
                .iter()
                .map(|reader| (reader.path.clone(), reader.bytes))
                .collect(),
        });
        if start.elapsed() >= options.duration {
            break;
        }
        thread::sleep(options.interval.min(options.duration - start.elapsed()));
    }

    Ok(Trace {
        schema: TRACE_SCHEMA.to_string(),
        captured_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        expect_motion: options.expect_motion,
        cursor_stalled: options.cursor_stalled,
        controllers,
        samples,
    })
}

pub fn write_json(path: &Path, trace: &Trace) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(trace).map_err(io::Error::other)?;
    fs::write(path, bytes)
}

pub fn read_json(path: &Path) -> io::Result<Trace> {
    let trace: Trace = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    if trace.schema != TRACE_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported trace schema '{}'", trace.schema),
        ));
    }
    Ok(trace)
}

fn open_event_nodes(controllers: &[Controller]) -> io::Result<Vec<EventReader>> {
    let mut readers = Vec::new();
    for path in controllers
        .iter()
        .flat_map(|controller| controller.event_nodes.iter())
    {
        let encoded = path.as_os_str().as_encoded_bytes();
        let cpath = CString::new(encoded)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in device path"))?;
        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::new(
                io::Error::last_os_error().kind(),
                format!(
                    "cannot open {}: {}",
                    path.display(),
                    io::Error::last_os_error()
                ),
            ));
        }
        readers.push(EventReader {
            path: path.clone(),
            file: unsafe { File::from_raw_fd(fd as RawFd) },
            bytes: 0,
        });
    }
    if readers.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Elan controller has no evdev nodes",
        ));
    }
    Ok(readers)
}

fn drain_events(readers: &mut [EventReader]) -> io::Result<()> {
    let mut buffer = [0_u8; 4096];
    for reader in readers {
        loop {
            match reader.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => reader.bytes = reader.bytes.saturating_add(count as u64),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}
