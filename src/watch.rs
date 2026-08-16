use crate::consumer::{BacklogObserver, ConsumerMonitor};
use crate::device::discover;
use crate::recover::rebind_controller;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const CONSUMER_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const CONSUMER_BACKLOG_GRACE: Duration = Duration::from_millis(750);
const RECOVERY_COOLDOWN: Duration = Duration::from_secs(5);

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

    pub fn observe_recovery(&mut self, id: &str, recoveries: u64) -> bool {
        let previous = self.previous(id);
        self.mark(id, recoveries);
        recoveries > previous
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
    let mut consumers = ConsumerMonitor::new(PathBuf::from("/proc"), CONSUMER_REFRESH_INTERVAL);
    let mut backlog = BacklogObserver::default();
    let mut consumer_count = None;
    let mut consumer_error_reported = false;
    let mut recovery_cooldown_until = None;

    loop {
        let now = Instant::now();
        match discover(sysfs_root) {
            Ok(controllers) => {
                let mut recovered_this_pass = false;
                for controller in &controllers {
                    let Some(count) = controller
                        .runtime_watchdog
                        .as_deref()
                        .and_then(recovery_count)
                    else {
                        continue;
                    };
                    if observer.observe_recovery(&controller.id, count) {
                        eprintln!(
                            "elan-guardian: kernel recovery observed on {}; rebinding controller",
                            controller.id
                        );
                        match rebind_controller(sysfs_root, &controller.id) {
                            Ok(recovered) => {
                                observer.mark(&controller.id, 0);
                                recovered_this_pass = true;
                                recovery_cooldown_until = Some(now + RECOVERY_COOLDOWN);
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

                if recovered_this_pass
                    || recovery_cooldown_until.is_some_and(|deadline| now < deadline)
                {
                    backlog.clear();
                    consumers.force_refresh();
                } else {
                    match consumers.refresh_if_due(&controllers, now) {
                        Ok(refreshed) => {
                            consumer_error_reported = false;
                            if refreshed && consumer_count != Some(consumers.len()) {
                                consumer_count = Some(consumers.len());
                                eprintln!(
                                    "elan-guardian: monitoring {} registered libinput ELAN fd(s)",
                                    consumers.len()
                                );
                            }
                        }
                        Err(error) => {
                            if !consumer_error_reported {
                                eprintln!(
                                    "elan-guardian: consumer backlog monitoring unavailable: {error}"
                                );
                                consumer_error_reported = true;
                            }
                        }
                    }

                    if !consumers.is_empty() {
                        let samples = consumers.readiness();
                        if let Some(stalled) =
                            backlog.observe(now, &samples, CONSUMER_BACKLOG_GRACE)
                        {
                            let controller = controllers.iter().find(|controller| {
                                controller.event_nodes.contains(&stalled.event_node)
                            });
                            if let Some(controller) = controller {
                                eprintln!(
                                    "elan-guardian: libinput consumer pid {} fd {} left {} unread for {} ms; rebinding {}",
                                    stalled.pid,
                                    stalled.remote_fd,
                                    stalled.event_node.display(),
                                    CONSUMER_BACKLOG_GRACE.as_millis(),
                                    controller.id
                                );
                                match rebind_controller(sysfs_root, &controller.id) {
                                    Ok(recovered) => {
                                        observer.mark(&controller.id, 0);
                                        recovery_cooldown_until = Some(now + RECOVERY_COOLDOWN);
                                        eprintln!(
                                            "elan-guardian: rebound {} ({})",
                                            recovered.id,
                                            recovered.event_nodes.join(", ")
                                        );
                                    }
                                    Err(error) => eprintln!(
                                        "elan-guardian: consumer-stall rebind of {} failed: {error}",
                                        controller.id
                                    ),
                                }
                                backlog.clear();
                                consumers.force_refresh();
                            }
                        }
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
        assert_eq!(
            recovery_count("enabled=1 recoveries=7 reports=4219 report_errors=0"),
            Some(7)
        );
        assert_eq!(recovery_count("enabled=0 recoveries=0\n"), Some(0));
        assert_eq!(recovery_count("unavailable"), None);
    }

    #[test]
    fn observes_each_recovery_generation_once() {
        let mut observer = RecoveryObserver::default();
        assert_eq!(observer.previous("7-0015"), 0);
        assert!(observer.observe_recovery("7-0015", 6));
        assert_eq!(observer.previous("7-0015"), 6);
        assert!(!observer.observe_recovery("7-0015", 6));
        assert!(observer.observe_recovery("7-0015", 7));
        observer.mark("7-0015", 0);
        assert_eq!(observer.previous("7-0015"), 0);
    }
}
