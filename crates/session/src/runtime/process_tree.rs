use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nix::unistd::Pid;
use parking_lot::Mutex;

const SNAPSHOT_CACHE_TTL: Duration = Duration::from_millis(20);

#[derive(Clone)]
struct CachedSnapshot {
    sequence: u64,
    captured_at: Instant,
    records: Arc<[ProcessRecord]>,
}

static PROCESS_SNAPSHOT_CACHE: OnceLock<Mutex<Option<CachedSnapshot>>> = OnceLock::new();
static NEXT_SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessIdentity {
    pid: i32,
    birth: String,
}

#[derive(Clone, Debug)]
struct ProcessRecord {
    identity: ProcessIdentity,
    parent_pid: i32,
    process_group_id: i32,
    zombie: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TrackedProcess {
    pub pid: Pid,
    pub process_group_id: Pid,
}

pub(super) struct ProcessTree {
    known: BTreeMap<i32, ProcessIdentity>,
    latest_snapshot_sequence: u64,
    scan_timeout: Duration,
    max_tracked_processes: usize,
}

impl ProcessTree {
    pub fn new(
        root_pid: Pid,
        scan_timeout: Duration,
        max_tracked_processes: usize,
    ) -> io::Result<Self> {
        let snapshot = process_snapshot(scan_timeout, true)?;
        let root_pid = root_pid.as_raw();
        let mut tree = Self {
            known: BTreeMap::new(),
            latest_snapshot_sequence: snapshot.sequence,
            scan_timeout,
            max_tracked_processes,
        };
        if let Some(root) = snapshot
            .records
            .iter()
            .find(|record| record.identity.pid == root_pid && !record.zombie)
        {
            tree.known.insert(root_pid, root.identity.clone());
        }
        let _ = tree.absorb_snapshot(&snapshot)?;
        Ok(tree)
    }

    pub fn refresh(&mut self) -> io::Result<Vec<TrackedProcess>> {
        // A global scan can begin before this tree's latest evidence and
        // publish afterward. Never let that older view prune a proven process.
        let snapshot = process_snapshot_after(
            self.scan_timeout,
            false,
            Some(self.latest_snapshot_sequence),
        )?;
        self.absorb_snapshot(&snapshot)
    }

    pub fn refresh_fresh(&mut self) -> io::Result<Vec<TrackedProcess>> {
        let snapshot = process_snapshot(self.scan_timeout, true)?;
        self.absorb_snapshot(&snapshot)
    }

    fn absorb_snapshot(&mut self, snapshot: &CachedSnapshot) -> io::Result<Vec<TrackedProcess>> {
        let processes = self.absorb(&snapshot.records)?;
        self.latest_snapshot_sequence = self.latest_snapshot_sequence.max(snapshot.sequence);
        Ok(processes)
    }

    fn absorb(&mut self, records: &[ProcessRecord]) -> io::Result<Vec<TrackedProcess>> {
        let records_by_pid = records
            .iter()
            .map(|record| (record.identity.pid, record))
            .collect::<HashMap<_, _>>();
        let mut verified = self
            .known
            .iter()
            .filter_map(|(pid, identity)| {
                records_by_pid
                    .get(pid)
                    .filter(|record| !record.zombie && record.identity == *identity)
                    .map(|_| *pid)
            })
            .collect::<BTreeSet<_>>();

        // An identity absent from a complete snapshot cannot become live again.
        // Pruning it prevents long-running interactive sessions from retaining
        // every short-lived command they have ever launched.
        self.known.retain(|pid, identity| {
            verified.contains(pid)
                && records_by_pid
                    .get(pid)
                    .is_some_and(|record| record.identity == *identity)
        });

        loop {
            let mut discovered = Vec::new();
            for record in records {
                if record.zombie
                    || verified.contains(&record.identity.pid)
                    || !verified.contains(&record.parent_pid)
                {
                    continue;
                }
                discovered.push(record.identity.clone());
            }
            if discovered.is_empty() {
                break;
            }
            for identity in discovered {
                verified.insert(identity.pid);
                self.known.insert(identity.pid, identity);
                if self.known.len() > self.max_tracked_processes {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "session descendant tracking bound exceeded",
                    ));
                }
            }
        }

        Ok(verified
            .into_iter()
            .filter_map(|pid| records_by_pid.get(&pid))
            .map(|record| TrackedProcess {
                pid: Pid::from_raw(record.identity.pid),
                process_group_id: Pid::from_raw(record.process_group_id),
            })
            .collect())
    }

    #[cfg(test)]
    fn with_root_for_test(root: ProcessRecord, max_tracked_processes: usize) -> Self {
        Self {
            known: BTreeMap::from([(root.identity.pid, root.identity)]),
            latest_snapshot_sequence: 1,
            scan_timeout: Duration::from_secs(1),
            max_tracked_processes,
        }
    }
}

fn process_snapshot(timeout: Duration, force: bool) -> io::Result<CachedSnapshot> {
    process_snapshot_after(timeout, force, None)
}

fn process_snapshot_after(
    timeout: Duration,
    force: bool,
    minimum_sequence: Option<u64>,
) -> io::Result<CachedSnapshot> {
    let started = Instant::now();
    let sequence = next_snapshot_sequence()?;
    let cache = PROCESS_SNAPSHOT_CACHE.get_or_init(|| Mutex::new(None));
    if !force {
        let Some(cached) = cache.try_lock_for(timeout) else {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process-tree snapshot cache lock exceeded its deadline",
            ));
        };
        if let Some(snapshot) = cached.as_ref() {
            if cached_snapshot_is_usable(snapshot, minimum_sequence) {
                return Ok(snapshot.clone());
            }
        }
    }
    let remaining = timeout.saturating_sub(started.elapsed());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "process-tree snapshot exceeded its deadline",
        ));
    }
    let records = Arc::from(scan_processes_uncached(remaining)?);
    let snapshot = CachedSnapshot {
        sequence,
        captured_at: Instant::now(),
        records,
    };
    // Scanning is intentionally outside the cache lock. Fresh lifecycle scans
    // must not queue behind unrelated sessions and exhaust their individual
    // safety deadlines. Cache publication is opportunistic because the result
    // itself is already complete and identity-checked.
    let remaining = timeout.saturating_sub(started.elapsed());
    if let Some(mut cache) = cache.try_lock_for(remaining) {
        // Concurrent scans finish out of order, so only their start sequence is
        // a reliable publication order.
        publish_snapshot(&mut cache, snapshot.clone());
    }
    Ok(snapshot)
}

fn next_snapshot_sequence() -> io::Result<u64> {
    // The counter is only an ordering token; the cache mutex publishes the
    // snapshot data, so no cross-thread memory ordering is required here.
    NEXT_SNAPSHOT_SEQUENCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |sequence| {
            sequence.checked_add(1)
        })
        .map_err(|_| io::Error::other("process-tree snapshot sequence exhausted"))
}

fn cached_snapshot_is_usable(snapshot: &CachedSnapshot, minimum_sequence: Option<u64>) -> bool {
    snapshot.captured_at.elapsed() <= SNAPSHOT_CACHE_TTL
        && minimum_sequence.is_none_or(|minimum| snapshot.sequence >= minimum)
}

fn publish_snapshot(cache: &mut Option<CachedSnapshot>, snapshot: CachedSnapshot) {
    let should_publish = cache
        .as_ref()
        .is_none_or(|cached| snapshot.sequence > cached.sequence);
    if should_publish {
        *cache = Some(snapshot);
    }
}

#[cfg(target_os = "linux")]
fn scan_processes_uncached(timeout: Duration) -> io::Result<Vec<ProcessRecord>> {
    const MAX_PROCESS_SNAPSHOT_RECORDS: usize = 262_144;

    let deadline = std::time::Instant::now() + timeout;
    let mut records = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process-tree snapshot exceeded its deadline",
            ));
        }
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        if let Some(record) = parse_linux_stat(pid, &stat) {
            records.push(record);
            if records.len() > MAX_PROCESS_SNAPSHOT_RECORDS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "process-tree snapshot exceeded its record bound",
                ));
            }
        }
    }
    Ok(records)
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_stat(expected_pid: i32, stat: &str) -> Option<ProcessRecord> {
    let command_end = stat.rfind(')')?;
    let prefix = stat.get(..command_end)?;
    let pid = prefix.split_once('(')?.0.trim().parse().ok()?;
    if pid != expected_pid {
        return None;
    }
    let fields = stat
        .get(command_end + 1..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    let state = fields.first()?.chars().next()?;
    let parent_pid = fields.get(1)?.parse().ok()?;
    let process_group_id = fields.get(2)?.parse().ok()?;
    let birth = fields.get(19)?.to_string();
    Some(ProcessRecord {
        identity: ProcessIdentity { pid, birth },
        parent_pid,
        process_group_id,
        zombie: state == 'Z',
    })
}

#[cfg(target_os = "macos")]
fn scan_processes_uncached(timeout: Duration) -> io::Result<Vec<ProcessRecord>> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::Instant,
    };

    const MAX_PS_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
    const MAX_PS_READ_BYTES: u64 = 8 * 1024 * 1024 + 1;
    const POLL_INTERVAL: Duration = Duration::from_millis(5);

    let mut child = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pgid=,stat=,lstart="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::other("ps stdout was unavailable"));
    };
    let (output_sender, output_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout
            .take(MAX_PS_READ_BYTES)
            .read_to_end(&mut output)
            .map(|_| output);
        let _ = output_sender.send(result);
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process-tree snapshot exceeded its deadline",
            ));
        }
        thread::sleep(POLL_INTERVAL.min(deadline - now));
    };
    if !status.success() {
        return Err(io::Error::other("ps process-tree snapshot failed"));
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    let output = output_receiver
        .recv_timeout(remaining)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ps output drain timed out"))??;
    if output.len() > MAX_PS_OUTPUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ps process-tree snapshot exceeded its size bound",
        ));
    }
    let output = String::from_utf8(output)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "ps output was not UTF-8"))?;
    Ok(output.lines().filter_map(parse_macos_ps_line).collect())
}

#[cfg(target_os = "macos")]
fn parse_macos_ps_line(line: &str) -> Option<ProcessRecord> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 9 {
        return None;
    }
    let pid = fields.first()?.parse().ok()?;
    let parent_pid = fields.get(1)?.parse().ok()?;
    let process_group_id = fields.get(2)?.parse().ok()?;
    let zombie = fields.get(3)?.starts_with('Z');
    let birth = fields.get(4..9)?.join(" ");
    Some(ProcessRecord {
        identity: ProcessIdentity { pid, birth },
        parent_pid,
        process_group_id,
        zombie,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn scan_processes_uncached(_timeout: Duration) -> io::Result<Vec<ProcessRecord>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "safe process-tree inspection is implemented for Linux and macOS",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pid: i32, parent_pid: i32, process_group_id: i32, birth: &str) -> ProcessRecord {
        ProcessRecord {
            identity: ProcessIdentity {
                pid,
                birth: birth.to_owned(),
            },
            parent_pid,
            process_group_id,
            zombie: false,
        }
    }

    fn snapshot(
        sequence: u64,
        captured_at: Instant,
        records: impl Into<Arc<[ProcessRecord]>>,
    ) -> CachedSnapshot {
        CachedSnapshot {
            sequence,
            captured_at,
            records: records.into(),
        }
    }

    #[test]
    fn out_of_order_scan_cannot_replace_a_newer_cached_snapshot() {
        let newer = snapshot(2, Instant::now(), vec![record(200, 1, 200, "newer")]);
        let older = snapshot(1, Instant::now(), vec![record(100, 1, 100, "older")]);
        let mut cache = Some(newer);

        publish_snapshot(&mut cache, older);

        let cached = cache.expect("newer cache entry should be retained");
        assert_eq!(cached.sequence, 2);
        assert_eq!(cached.records[0].identity.pid, 200);
    }

    #[test]
    fn cached_snapshot_from_before_latest_tree_evidence_is_not_usable() {
        let stale = snapshot(1, Instant::now(), Vec::<ProcessRecord>::new());

        assert!(!cached_snapshot_is_usable(&stale, Some(2)));
    }

    #[test]
    fn fresh_snapshot_advances_the_tree_cache_floor() {
        let root = record(100, 1, 100, "root");
        let child = record(101, 100, 101, "child");
        let mut tree = ProcessTree::with_root_for_test(root.clone(), 8);
        let newer = snapshot(3, Instant::now(), vec![root.clone(), child]);

        tree.absorb_snapshot(&newer)
            .expect("newer process-tree evidence should be accepted");

        let stale = snapshot(2, Instant::now(), vec![root]);
        assert_eq!(tree.latest_snapshot_sequence, 3);
        assert!(tree.known.contains_key(&101));
        assert!(!cached_snapshot_is_usable(
            &stale,
            Some(tree.latest_snapshot_sequence)
        ));
    }

    #[test]
    fn only_proven_descendants_are_retained_across_group_changes() {
        let root = record(100, 1, 100, "root");
        let mut tree = ProcessTree::with_root_for_test(root.clone(), 8);
        let tracked = tree
            .absorb(&[
                root,
                record(101, 100, 101, "child"),
                record(102, 101, 102, "grandchild"),
                record(200, 1, 200, "unrelated"),
            ])
            .expect("bounded synthetic process tree should be accepted");

        assert_eq!(
            tracked
                .iter()
                .map(|process| process.pid.as_raw())
                .collect::<Vec<_>>(),
            [100, 101, 102]
        );
        assert!(!tree.known.contains_key(&200));
    }

    #[test]
    fn reused_pid_is_not_followed_without_current_parentage_proof() {
        let root = record(100, 1, 100, "root");
        let mut tree = ProcessTree::with_root_for_test(root.clone(), 8);
        tree.absorb(&[root, record(101, 100, 101, "original")])
            .expect("original descendant should be tracked");

        let tracked = tree
            .absorb(&[
                record(101, 1, 101, "reused"),
                record(102, 101, 102, "not-ours"),
            ])
            .expect("reused unrelated processes should be ignored");

        assert!(tracked.is_empty());
        assert!(tree.known.is_empty());
    }

    #[test]
    fn zombie_identity_is_not_returned_as_a_live_signal_target() {
        let root = record(100, 1, 100, "root");
        let child = record(101, 100, 100, "child");
        let mut tree = ProcessTree::with_root_for_test(root.clone(), 8);
        tree.absorb(&[root.clone(), child.clone()])
            .expect("live child should initially be tracked");
        let mut zombie_child = child;
        zombie_child.zombie = true;

        let tracked = tree
            .absorb(&[root, zombie_child])
            .expect("zombie snapshot should remain valid");

        assert_eq!(
            tracked
                .iter()
                .map(|process| process.pid.as_raw())
                .collect::<Vec<_>>(),
            [100]
        );
        assert!(!tree.known.contains_key(&101));
    }

    #[test]
    fn linux_stat_parser_handles_spaces_and_parentheses_in_command_name() {
        let stat = "42 (name with ) paren) S 7 42 42 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 12345";
        let parsed = parse_linux_stat(42, stat).expect("synthetic stat should parse");

        assert_eq!(parsed.parent_pid, 7);
        assert_eq!(parsed.process_group_id, 42);
        assert_eq!(parsed.identity.birth, "12345");
    }
}
