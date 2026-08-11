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
        let recovered = write_control(&in_place, "1").and_then(|()| {
            wait_until(Duration::from_secs(3), || {
                !event_nodes(sysfs_root, id).is_empty()
            })
        });
        if recovered.is_ok() {
            return Ok(RecoveredController {
                id: id.into(),
                method: RecoverMethod::InPlace,
                event_nodes: event_nodes(sysfs_root, id),
            });
        }
    }

    rebind_controller(sysfs_root, id)
}

pub fn rebind_controller(sysfs_root: &Path, id: &str) -> io::Result<RecoveredController> {
    if !valid_i2c_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid I2C device identifier '{id}'"),
        ));
    }
    let driver = sysfs_root.join(DRIVER_RELATIVE_PATH);
    let canonical_driver = fs::canonicalize(&driver)?;
    if sysfs_root
        .join(DEVICES_RELATIVE_PATH)
        .join(id)
        .join("driver")
        .exists()
    {
        write_control(&driver.join("unbind"), id)?;
    }
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
    let command = format!("{value}\n");
    control.write_all(command.as_bytes())
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

#[cfg(test)]
mod tests {
    use super::{recover, RecoverMethod};
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn writes_one_complete_in_place_command() {
        let root =
            std::env::temp_dir().join(format!("elan-guardian-recover-test-{}", std::process::id()));
        let driver = root.join("bus/i2c/drivers/elan_i2c");
        let device = root.join("bus/i2c/devices/7-0015");
        fs::create_dir_all(&driver).unwrap();
        fs::create_dir_all(device.join("input/input0/event1")).unwrap();
        symlink(&driver, device.join("driver")).unwrap();
        symlink(&device, driver.join("7-0015")).unwrap();
        fs::write(device.join("recover"), []).unwrap();

        let recovered = recover(&root, "7-0015").unwrap();
        assert_eq!(recovered.method, RecoverMethod::InPlace);
        assert_eq!(fs::read_to_string(device.join("recover")).unwrap(), "1\n");

        fs::remove_dir_all(root).unwrap();
    }
}
