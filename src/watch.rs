use crate::device::discover;
use crate::recover::rebind_controller;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct RecoveryObserver {
    seen: HashMap<String, u64>,
}

impl RecoveryObserver {
    pub fn previous(&self, id: &str) -> u64 {
        self.seen.get(id).copied().unwrap_or(0)
    }

    pub fn mark(&mut self, id: &str, recoveries: u64) {
        self.seen.insert(id.to_owned(), recoveries);
    }
}

pub fn recovery_count(status: &str) -> Option<u64> {
    status
        .split_ascii_whitespace()
        .find_map(|field| field.strip_prefix("recoveries="))?
        .parse()
        .ok()
}

pub fn monitor(sysfs_root: &Path, interval: Duration) -> io::Result<()> {
    let mut observer = RecoveryObserver::default();

    loop {
        match discover(sysfs_root) {
            Ok(controllers) => {
                for controller in controllers {
                    let Some(count) = controller
                        .runtime_watchdog
                        .as_deref()
                        .and_then(recovery_count)
                    else {
                        continue;
                    };
                    let previous = observer.previous(&controller.id);
                    if count > previous {
                        eprintln!(
                            "elan-guardian: kernel recovery observed on {}; rebinding controller",
                            controller.id
                        );
                        match rebind_controller(sysfs_root, &controller.id) {
                            Ok(recovered) => {
                                observer.mark(&controller.id, 0);
                                eprintln!(
                                    "elan-guardian: rebound {} ({})",
                                    recovered.id,
                                    recovered.event_nodes.join(", ")
                                );
                            }
                            Err(error) => {
                                eprintln!(
                                    "elan-guardian: rebind of {} failed: {error}",
                                    controller.id
                                );
                            }
                        }
                    } else {
                        observer.mark(&controller.id, count);
                    }
                }
            }
            Err(error) => eprintln!("elan-guardian: controller scan failed: {error}"),
        }
        thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::{recovery_count, RecoveryObserver};

    #[test]
    fn parses_runtime_watchdog_status() {
        assert_eq!(recovery_count("enabled=1 recoveries=6"), Some(6));
        assert_eq!(recovery_count("enabled=0 recoveries=0\n"), Some(0));
        assert_eq!(recovery_count("unavailable"), None);
    }

    #[test]
    fn observes_each_recovery_generation_once() {
        let mut observer = RecoveryObserver::default();
        assert_eq!(observer.previous("7-0015"), 0);
        observer.mark("7-0015", 6);
        assert_eq!(observer.previous("7-0015"), 6);
        observer.mark("7-0015", 0);
        assert_eq!(observer.previous("7-0015"), 0);
    }
}
