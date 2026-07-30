use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DRIVER_RELATIVE_PATH: &str = "bus/i2c/drivers/elan_i2c";
pub const DEVICES_RELATIVE_PATH: &str = "bus/i2c/devices";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Controller {
    pub id: String,
    pub sysfs_path: PathBuf,
    pub product_id: Option<String>,
    pub firmware_version: Option<String>,
    pub sample_version: Option<String>,
    pub iap_version: Option<String>,
    pub mode: Option<String>,
    pub runtime_watchdog: Option<String>,
    pub irq: Option<u32>,
    pub event_nodes: Vec<PathBuf>,
}

pub fn discover(sysfs_root: &Path) -> io::Result<Vec<Controller>> {
    let driver = sysfs_root.join(DRIVER_RELATIVE_PATH);
    let canonical_driver = fs::canonicalize(&driver)?;
    let mut controllers = Vec::new();

    for entry in fs::read_dir(&driver)? {
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_i2c_id(&id) || !uses_driver(sysfs_root, &id, &canonical_driver) {
            continue;
        }
        let path = sysfs_root.join(DEVICES_RELATIVE_PATH).join(&id);
        controllers.push(Controller {
            id: id.clone(),
            sysfs_path: path.clone(),
            // These elan_i2c attributes issue live transport requests when read.
            // Runtime discovery must stay passive: polling them can repeatedly
            // exercise a wedged SMBus controller and make the input outage worse.
            product_id: None,
            firmware_version: None,
            sample_version: None,
            iap_version: None,
            mode: None,
            runtime_watchdog: read_trimmed(path.join("runtime_watchdog")),
            irq: read_trimmed(path.join("irq")).and_then(|value| value.parse().ok()),
            event_nodes: event_nodes(&path),
        });
    }

    controllers.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(controllers)
}

pub fn affected_thinkpad_p53(sysfs_root: &Path) -> bool {
    let vendor = read_trimmed(sysfs_root.join("class/dmi/id/sys_vendor"));
    let version = read_trimmed(sysfs_root.join("class/dmi/id/product_version"));
    vendor.as_deref() == Some("LENOVO")
        && version
            .as_deref()
            .is_some_and(|value| value.starts_with("ThinkPad P53"))
}

pub fn valid_i2c_id(id: &str) -> bool {
    let Some((bus, address)) = id.split_once('-') else {
        return false;
    };
    !bus.is_empty()
        && bus.bytes().all(|byte| byte.is_ascii_digit())
        && address.len() == 4
        && address.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn uses_driver(sysfs_root: &Path, id: &str, canonical_driver: &Path) -> bool {
    fs::canonicalize(
        sysfs_root
            .join(DEVICES_RELATIVE_PATH)
            .join(id)
            .join("driver"),
    )
    .is_ok_and(|path| path == canonical_driver)
}

fn event_nodes(controller: &Path) -> Vec<PathBuf> {
    let Ok(inputs) = fs::read_dir(controller.join("input")) else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    for input in inputs.flatten() {
        let Ok(entries) = fs::read_dir(input.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.strip_prefix("event").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                nodes.push(PathBuf::from("/dev/input").join(name));
            }
        }
    }
    nodes.sort();
    nodes.dedup();
    nodes
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::{discover, valid_i2c_id};
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn accepts_only_canonical_i2c_identifiers() {
        assert!(valid_i2c_id("7-0015"));
        assert!(valid_i2c_id("12-00AF"));
        assert!(!valid_i2c_id("../elan_i2c"));
        assert!(!valid_i2c_id("7-15"));
        assert!(!valid_i2c_id("i2c-7-0015"));
    }

    #[test]
    fn runtime_discovery_does_not_probe_transport_metadata() {
        let root = std::env::temp_dir().join(format!(
            "elan-guardian-discovery-test-{}",
            std::process::id()
        ));
        let driver = root.join("bus/i2c/drivers/elan_i2c");
        let device = root.join("bus/i2c/devices/7-0015");
        fs::create_dir_all(device.join("input/input0/event1")).unwrap();
        fs::create_dir_all(&driver).unwrap();
        symlink(&driver, device.join("driver")).unwrap();
        symlink(&device, driver.join("7-0015")).unwrap();
        fs::write(device.join("firmware_version"), "116\n").unwrap();
        fs::write(device.join("runtime_watchdog"), "enabled=1 recoveries=0\n").unwrap();

        let controllers = discover(&root).unwrap();
        assert_eq!(controllers.len(), 1);
        assert_eq!(controllers[0].firmware_version, None);
        assert_eq!(
            controllers[0].runtime_watchdog.as_deref(),
            Some("enabled=1 recoveries=0")
        );

        fs::remove_dir_all(root).unwrap();
    }
}
