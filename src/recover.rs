use crate::device::{
    discover, uses_driver, valid_i2c_id, DEVICES_RELATIVE_PATH, DRIVER_RELATIVE_PATH,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoverMethod {
    InPlace,
    Rebind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveredController {
    pub id: String,
    pub method: RecoverMethod,
    pub event_nodes: Vec<String>,
}

pub fn recover(sysfs_root: &Path, id: &str) -> io::Result<RecoveredController> {
    if !valid_i2c_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid I2C device identifier '{id}'"),
        ));
    }
    let controllers = discover(sysfs_root)?;
    if !controllers.iter().any(|controller| controller.id == id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{id} is not bound to elan_i2c"),
        ));
    }

    let controller = sysfs_root.join(DEVICES_RELATIVE_PATH).join(id);
    let in_place = controller.join("recover");
    if in_place.exists() {
        write_control(&in_place, "1")?;
        wait_until(Duration::from_secs(3), || {
            !event_nodes(sysfs_root, id).is_empty()
        })?;
        return Ok(RecoveredController {
            id: id.into(),
            method: RecoverMethod::InPlace,
            event_nodes: event_nodes(sysfs_root, id),
        });
    }

    rebind(sysfs_root, id)
}

fn rebind(sysfs_root: &Path, id: &str) -> io::Result<RecoveredController> {
    let driver = sysfs_root.join(DRIVER_RELATIVE_PATH);
    let canonical_driver = fs::canonicalize(&driver)?;
    write_control(&driver.join("unbind"), id)?;
    wait_until(Duration::from_secs(1), || {
        !sysfs_root
            .join(DEVICES_RELATIVE_PATH)
            .join(id)
            .join("driver")
            .exists()
    })?;
    thread::sleep(Duration::from_millis(100));

    let delays = [0, 100, 250, 500, 1000];
    let mut last_error = None;
    for delay in delays {
        thread::sleep(Duration::from_millis(delay));
        match write_control(&driver.join("bind"), id) {
            Ok(()) => {
                last_error = None;
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    if let Some(error) = last_error {
        return Err(io::Error::new(
            error.kind(),
            format!("ELAN {id} was unbound but could not be rebound: {error}"),
        ));
    }
    wait_until(Duration::from_secs(3), || {
        uses_driver(sysfs_root, id, &canonical_driver)
    })?;
    wait_until(Duration::from_secs(3), || {
        !event_nodes(sysfs_root, id).is_empty()
    })?;
    Ok(RecoveredController {
        id: id.into(),
        method: RecoverMethod::Rebind,
        event_nodes: event_nodes(sysfs_root, id),
    })
}

fn write_control(path: &Path, value: &str) -> io::Result<()> {
    let mut control = OpenOptions::new().write(true).open(path)?;
    writeln!(control, "{value}")
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
            "timed out waiting for the Elan controller",
        )
    })
}

fn event_nodes(sysfs_root: &Path, id: &str) -> Vec<String> {
    discover(sysfs_root)
        .ok()
        .and_then(|controllers| controllers.into_iter().find(|device| device.id == id))
        .map(|controller| {
            controller
                .event_nodes
                .into_iter()
                .filter_map(|node| {
                    node.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}
