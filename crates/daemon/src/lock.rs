use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use rustix::fs::FlockOperation;

use crate::DaemonError;

pub(crate) struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub(crate) fn acquire(path: &Path) -> Result<Self, DaemonError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .map_err(|error| DaemonError::io("open instance lock", path, error))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| DaemonError::io("secure instance lock", path, error))?;

        if let Err(error) = rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive) {
            let error = io::Error::from(error);
            if error.kind() == io::ErrorKind::WouldBlock {
                return Err(DaemonError::AlreadyRunning {
                    path: path.to_path_buf(),
                });
            }
            return Err(DaemonError::io("lock daemon instance", path, error));
        }

        Ok(Self { _file: file })
    }
}
