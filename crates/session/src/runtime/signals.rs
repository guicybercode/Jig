use nix::{
    errno::Errno,
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

use crate::SessionError;

use super::{SessionRuntime, process_tree::TrackedProcess};

#[derive(Debug, Eq, PartialEq)]
struct SignalPlan {
    process_group: Option<Pid>,
    processes: Vec<Pid>,
}

impl SessionRuntime {
    pub(super) fn signal_process_group(
        &self,
        signal: Signal,
        signal_name: &'static str,
    ) -> Result<(), SessionError> {
        // A fresh snapshot is mandatory before using either a saved PGID or a
        // PID. `ProcessTree` verifies process birth identity and excludes
        // zombies, so a stale/reused numeric identifier is never trusted by
        // itself.
        let processes = self.tracked_processes()?;
        let saved_group = *self.process_group_id.lock();
        let plan = signal_plan(saved_group, &processes);
        if saved_group.is_some() && plan.process_group.is_none() {
            self.retire_process_group();
        }

        let mut first_error = None;
        if let Some(process_group) = plan.process_group {
            match killpg(process_group, signal) {
                Ok(()) => {}
                Err(Errno::ESRCH) => self.clear_process_group(process_group),
                Err(source) => {
                    // Do not retain a group number after its verified snapshot:
                    // a later retry must establish a new live identity first.
                    self.clear_process_group(process_group);
                    first_error = Some(SessionError::Signal {
                        session_id: self.id,
                        signal: signal_name,
                        source,
                    });
                }
            }
        }

        // Individual delivery covers proven descendants that changed process
        // groups. Signaling every identity also keeps group delivery an
        // optimization, never the sole cleanup mechanism.
        for process in plan.processes {
            if let Err(source) = kill(process, signal) {
                if source != Errno::ESRCH && first_error.is_none() {
                    first_error = Some(SessionError::Signal {
                        session_id: self.id,
                        signal: signal_name,
                        source,
                    });
                }
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(super) fn process_group_exists(&self) -> Result<bool, SessionError> {
        let processes = self.tracked_processes()?;
        let saved_group = *self.process_group_id.lock();
        if saved_group.is_some() && signal_plan(saved_group, &processes).process_group.is_none() {
            self.retire_process_group();
        }
        // A fresh identity-checked process scan is the existence proof. A
        // signal-0 probe of a saved PGID could instead observe an unrelated
        // group that reused the same number.
        Ok(!processes.is_empty())
    }

    pub(super) fn tracked_processes(&self) -> Result<Vec<TrackedProcess>, SessionError> {
        self.process_tree
            .lock()
            .refresh_fresh()
            .map_err(|source| SessionError::ProcessInspection {
                session_id: self.id,
                source,
            })
    }

    pub(super) fn cached_tracked_processes(&self) -> Result<Vec<TrackedProcess>, SessionError> {
        self.process_tree
            .lock()
            .refresh()
            .map_err(|source| SessionError::ProcessInspection {
                session_id: self.id,
                source,
            })
    }

    pub(super) fn retire_process_group(&self) {
        self.process_group_id.lock().take();
    }

    fn clear_process_group(&self, expected: Pid) {
        let mut process_group_id = self.process_group_id.lock();
        if *process_group_id == Some(expected) {
            *process_group_id = None;
        }
    }
}

fn signal_plan(saved_group: Option<Pid>, processes: &[TrackedProcess]) -> SignalPlan {
    let process_group = saved_group.filter(|group| {
        processes
            .iter()
            .any(|process| process.process_group_id == *group)
    });
    SignalPlan {
        process_group,
        processes: processes.iter().map(|process| process.pid).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: i32, process_group_id: i32) -> TrackedProcess {
        TrackedProcess {
            pid: Pid::from_raw(pid),
            process_group_id: Pid::from_raw(process_group_id),
        }
    }

    #[test]
    fn stale_saved_group_is_never_selected_for_signaling() {
        let plan = signal_plan(Some(Pid::from_raw(100)), &[process(101, 101)]);

        assert_eq!(plan.process_group, None);
        assert_eq!(plan.processes, [Pid::from_raw(101)]);
    }

    #[test]
    fn live_identity_in_saved_group_authorizes_group_signal() {
        let plan = signal_plan(
            Some(Pid::from_raw(100)),
            &[process(100, 100), process(101, 101)],
        );

        assert_eq!(plan.process_group, Some(Pid::from_raw(100)));
        assert_eq!(plan.processes, [Pid::from_raw(100), Pid::from_raw(101)]);
    }
}
