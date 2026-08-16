use crate::device::Controller;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConsumerKey {
    pub pid: u32,
    pub remote_fd: RawFd,
    pub event_node: PathBuf,
}

#[derive(Debug)]
struct ConsumerFd {
    key: ConsumerKey,
    local_fd: OwnedFd,
}

#[derive(Debug)]
pub struct ConsumerMonitor {
    proc_root: PathBuf,
    consumers: Vec<ConsumerFd>,
    refreshed_at: Option<Instant>,
    refresh_interval: Duration,
}

impl ConsumerMonitor {
    pub fn new(proc_root: PathBuf, refresh_interval: Duration) -> Self {
        Self {
            proc_root,
            consumers: Vec::new(),
            refreshed_at: None,
            refresh_interval,
        }
    }

    pub fn refresh_if_due(&mut self, controllers: &[Controller], now: Instant) -> io::Result<bool> {
        if self
            .refreshed_at
            .is_some_and(|last| now.duration_since(last) < self.refresh_interval)
        {
            return Ok(false);
        }
        self.refresh(controllers)?;
        self.refreshed_at = Some(now);
        Ok(true)
    }

    pub fn force_refresh(&mut self) {
        self.consumers.clear();
        self.refreshed_at = None;
    }

    pub fn len(&self) -> usize {
        self.consumers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.consumers.is_empty()
    }

    pub fn readiness(&mut self) -> Vec<(ConsumerKey, bool)> {
        let mut samples = Vec::with_capacity(self.consumers.len());
        self.consumers.retain(
            |consumer| match fd_readiness(consumer.local_fd.as_raw_fd()) {
                Ok(FdReadiness::Ready) => {
                    samples.push((consumer.key.clone(), true));
                    true
                }
                Ok(FdReadiness::Idle) => {
                    samples.push((consumer.key.clone(), false));
                    true
                }
                Ok(FdReadiness::Dead) | Err(_) => false,
            },
        );
        samples
    }

    fn refresh(&mut self, controllers: &[Controller]) -> io::Result<()> {
        let event_nodes: HashSet<PathBuf> = controllers
            .iter()
            .flat_map(|controller| controller.event_nodes.iter().cloned())
            .collect();
        let discovered = discover_consumers(&self.proc_root, &event_nodes)?;
        self.consumers = discovered;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct BacklogObserver {
    ready_since: HashMap<ConsumerKey, Instant>,
}

impl BacklogObserver {
    pub fn observe(
        &mut self,
        now: Instant,
        samples: &[(ConsumerKey, bool)],
        grace: Duration,
    ) -> Option<ConsumerKey> {
        let present: HashSet<&ConsumerKey> = samples.iter().map(|(key, _)| key).collect();
        self.ready_since.retain(|key, _| present.contains(key));

        for (key, ready) in samples {
            if !ready {
                self.ready_since.remove(key);
                continue;
            }
            let since = self.ready_since.entry(key.clone()).or_insert(now);
            if now.duration_since(*since) >= grace {
                return Some(key.clone());
            }
        }
        None
    }

    pub fn clear(&mut self) {
        self.ready_since.clear();
    }
}

fn discover_consumers(
    proc_root: &Path,
    event_nodes: &HashSet<PathBuf>,
) -> io::Result<Vec<ConsumerFd>> {
    if event_nodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut consumers = Vec::new();
    let mut permission_denials = 0_u32;
    for process in fs::read_dir(proc_root)? {
        let process = match process {
            Ok(process) => process,
            Err(_) => continue,
        };
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process_path = process.path();
        if !maps_libinput(&process_path) {
            continue;
        }
        let registered = registered_epoll_fds(&process_path);
        if registered.is_empty() {
            continue;
        }
        let fd_dir = process_path.join("fd");
        let Ok(fds) = fs::read_dir(fd_dir) else {
            continue;
        };
        for fd_entry in fds.flatten() {
            let Some(remote_fd) = fd_entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<RawFd>().ok())
            else {
                continue;
            };
            if !registered.contains(&remote_fd) {
                continue;
            }
            let Ok(target) = fs::read_link(fd_entry.path()) else {
                continue;
            };
            if !event_nodes.contains(&target) {
                continue;
            }
            match duplicate_remote_fd(pid, remote_fd) {
                Ok(local_fd) => consumers.push(ConsumerFd {
                    key: ConsumerKey {
                        pid,
                        remote_fd,
                        event_node: target,
                    },
                    local_fd,
                }),
                Err(error)
                    if matches!(error.raw_os_error(), Some(libc::ESRCH) | Some(libc::EBADF)) => {}
                Err(error)
                    if matches!(error.raw_os_error(), Some(libc::EPERM) | Some(libc::EACCES)) =>
                {
                    permission_denials = permission_denials.saturating_add(1);
                }
                Err(error) => return Err(error),
            }
        }
    }
    if consumers.is_empty() && permission_denials != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "pidfd_getfd cannot inspect the registered libinput consumer",
        ));
    }
    consumers.sort_by(|left, right| {
        (left.key.pid, left.key.remote_fd).cmp(&(right.key.pid, right.key.remote_fd))
    });
    Ok(consumers)
}

fn maps_libinput(process_path: &Path) -> bool {
    fs::read_to_string(process_path.join("maps"))
        .is_ok_and(|maps| maps.lines().any(|line| line.contains("libinput.so")))
}

fn registered_epoll_fds(process_path: &Path) -> HashSet<RawFd> {
    let mut registered = HashSet::new();
    let Ok(entries) = fs::read_dir(process_path.join("fdinfo")) else {
        return registered;
    };
    for entry in entries.flatten() {
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in contents.lines() {
            let mut fields = line.split_ascii_whitespace();
            if fields.next() == Some("tfd:") {
                if let Some(fd) = fields.next().and_then(|value| value.parse().ok()) {
                    registered.insert(fd);
                }
            }
        }
    }
    registered
}

fn duplicate_remote_fd(pid: u32, remote_fd: RawFd) -> io::Result<OwnedFd> {
    let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) as RawFd };
    if pidfd < 0 {
        return Err(io::Error::last_os_error());
    }
    let local_fd = unsafe { libc::syscall(libc::SYS_pidfd_getfd, pidfd, remote_fd, 0) as RawFd };
    unsafe {
        libc::close(pidfd);
    }
    if local_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(local_fd) })
}

enum FdReadiness {
    Idle,
    Ready,
    Dead,
}

fn fd_readiness(fd: RawFd) -> io::Result<FdReadiness> {
    let mut pollfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut pollfd, 1, 0) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if pollfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Ok(FdReadiness::Dead);
    }
    if pollfd.revents & libc::POLLIN != 0 {
        Ok(FdReadiness::Ready)
    } else {
        Ok(FdReadiness::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::{registered_epoll_fds, BacklogObserver, ConsumerKey};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn key() -> ConsumerKey {
        ConsumerKey {
            pid: 42,
            remote_fd: 7,
            event_node: PathBuf::from("/dev/input/event9"),
        }
    }

    #[test]
    fn parses_eventpoll_registration_targets() {
        let root = std::env::temp_dir().join(format!(
            "elan-guardian-consumer-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("fdinfo")).unwrap();
        fs::write(
            root.join("fdinfo/4"),
            "pos:\t0\ntfd:       53 events:       19 data: 35\n",
        )
        .unwrap();
        let fds = registered_epoll_fds(&root);
        assert!(fds.contains(&53));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn requires_continuous_backlog_for_the_full_grace_period() {
        let mut observer = BacklogObserver::default();
        let start = Instant::now();
        let grace = Duration::from_millis(750);
        assert_eq!(observer.observe(start, &[(key(), true)], grace), None);
        assert_eq!(
            observer.observe(start + Duration::from_millis(749), &[(key(), true)], grace),
            None
        );
        assert_eq!(
            observer.observe(start + grace, &[(key(), true)], grace),
            Some(key())
        );
    }

    #[test]
    fn an_idle_sample_clears_a_partial_backlog() {
        let mut observer = BacklogObserver::default();
        let start = Instant::now();
        let grace = Duration::from_millis(750);
        assert_eq!(observer.observe(start, &[(key(), true)], grace), None);
        assert_eq!(
            observer.observe(start + Duration::from_millis(500), &[(key(), false)], grace),
            None
        );
        assert_eq!(
            observer.observe(start + Duration::from_secs(2), &[(key(), true)], grace),
            None
        );
    }
}
