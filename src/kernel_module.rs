use crate::device::{discover, uses_driver, DEVICES_RELATIVE_PATH, DRIVER_RELATIVE_PATH};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

const MODULE_NAME: &str = "elan_i2c";
const MODINFO: &str = "modinfo";
const MODPROBE: &str = "modprobe";
const MODULE_SRCVERSION_RELATIVE_PATH: &str = "module/elan_i2c/srcversion";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationResult {
    Current {
        srcversion: String,
    },
    Activated {
        srcversion: String,
        controllers: Vec<String>,
    },
}

pub fn activate(sysfs_root: &Path) -> io::Result<ActivationResult> {
    let installed = installed_srcversion()?;
    let loaded = read_trimmed(sysfs_root.join(MODULE_SRCVERSION_RELATIVE_PATH));
    if !activation_needed(loaded.as_deref(), &installed) {
        return Ok(ActivationResult::Current {
            srcversion: installed,
        });
    }

    let controllers = if loaded.is_some() {
        discover(sysfs_root)?
            .into_iter()
            .map(|controller| controller.id)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if loaded.is_some() {
        unbind_controllers(sysfs_root, &controllers)?;
        if let Err(error) = run_modprobe(&["-r", MODULE_NAME]) {
            rebind_controllers(sysfs_root, &controllers);
            return Err(error);
        }
    }

    run_modprobe(&[MODULE_NAME])?;
    wait_until(Duration::from_secs(5), || {
        read_trimmed(sysfs_root.join(MODULE_SRCVERSION_RELATIVE_PATH)).as_deref()
            == Some(installed.as_str())
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "loaded {MODULE_NAME} does not match installed srcversion {installed}: {error}"
            ),
        )
    })?;

    let canonical_driver = fs::canonicalize(sysfs_root.join(DRIVER_RELATIVE_PATH))?;
    for id in &controllers {
        wait_until(Duration::from_secs(5), || {
            uses_driver(sysfs_root, id, &canonical_driver)
        })
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("{id} did not bind to the activated {MODULE_NAME}: {error}"),
            )
        })?;
        wait_until(Duration::from_secs(5), || {
            discover(sysfs_root).is_ok_and(|devices| {
                devices
                    .iter()
                    .any(|device| device.id == *id && !device.event_nodes.is_empty())
            })
        })
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("{id} did not republish its input devices: {error}"),
            )
        })?;
    }

    Ok(ActivationResult::Activated {
        srcversion: installed,
        controllers,
    })
}

fn activation_needed(loaded: Option<&str>, installed: &str) -> bool {
    loaded != Some(installed)
}

fn installed_srcversion() -> io::Result<String> {
    let output = Command::new(MODINFO)
        .args(["-F", "srcversion", MODULE_NAME])
        .output()?;
    successful_output(MODINFO, output)
}

fn run_modprobe(args: &[&str]) -> io::Result<()> {
    let output = Command::new(MODPROBE).args(args).output()?;
    ensure_success(MODPROBE, &output)
}

fn successful_output(program: &str, output: Output) -> io::Result<String> {
    ensure_success(program, &output)?;
    let value = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{program} returned an empty module identity"),
        ));
    }
    Ok(value)
}

fn ensure_success(program: &str, output: &Output) -> io::Result<()> {
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(io::Error::other(if detail.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            format!("{program} failed: {detail}")
        }));
    }
    Ok(())
}

fn unbind_controllers(sysfs_root: &Path, controllers: &[String]) -> io::Result<()> {
    let unbind = sysfs_root.join(DRIVER_RELATIVE_PATH).join("unbind");
    let mut last_error = None;
    for id in controllers {
        if !sysfs_root
            .join(DEVICES_RELATIVE_PATH)
            .join(id)
            .join("driver")
            .exists()
        {
            continue;
        }
        if let Err(error) = write_control(&unbind, id) {
            last_error = Some(io::Error::new(
                error.kind(),
                format!("could not unbind {id} before module activation: {error}"),
            ));
        }
    }
    if let Some(error) = last_error {
        rebind_controllers(sysfs_root, controllers);
        return Err(error);
    }
    Ok(())
}

fn rebind_controllers(sysfs_root: &Path, controllers: &[String]) {
    let bind = sysfs_root.join(DRIVER_RELATIVE_PATH).join("bind");
    for id in controllers {
        if !sysfs_root
            .join(DEVICES_RELATIVE_PATH)
            .join(id)
            .join("driver")
            .exists()
        {
            let _ = write_control(&bind, id);
        }
    }
}

fn write_control(path: &Path, value: &str) -> io::Result<()> {
    let control = OpenOptions::new().write(true).open(path)?;
    write_command(control, value)
}

fn write_command(mut control: impl Write, value: &str) -> io::Result<()> {
    let command = format!("{value}\n");
    control.write_all(command.as_bytes())
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    predicate().then_some(()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "timed out waiting for kernel module activation",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{activation_needed, write_command};
    use std::io::{self, Write};

    #[derive(Default)]
    struct RecordingWriter {
        calls: usize,
        bytes: Vec<u8>,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn reloads_only_when_the_loaded_module_is_absent_or_stale() {
        assert!(activation_needed(None, "NEW"));
        assert!(activation_needed(Some("OLD"), "NEW"));
        assert!(!activation_needed(Some("NEW"), "NEW"));
    }

    #[test]
    fn sends_each_sysfs_command_in_one_write() {
        let mut writer = RecordingWriter::default();
        write_command(&mut writer, "7-0015").unwrap();
        assert_eq!(writer.calls, 1);
        assert_eq!(writer.bytes, b"7-0015\n");
    }
}
