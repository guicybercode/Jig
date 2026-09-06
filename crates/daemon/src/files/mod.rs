//! Descriptor-relative local text files; all roots come from daemon metadata.

mod attributes;
mod descriptor;
mod error;
mod listing;
mod metadata;
mod write;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::ApiError;
use cli_master_core::wire::{
    FileListRequest, FileReadRequest, FileReadResponse, FileTarget, FileWriteRequest,
    FileWriteResponse, method,
};
use rustix::fs::fstat;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use descriptor::{
    Identity, identity, open_directory, open_root, read_leaf, revalidate_namespace, split_file_path,
};
use error::{api, invalid_input, io_error, target_changed};

/// A root obtained from a currently registered project, session or worktree.
pub(crate) struct ResolvedFileTarget {
    pub(crate) root: PathBuf,
}

/// Runtime authority supplies metadata resolution and its worktree-removal lease.
pub(crate) trait FileTargetAccess {
    fn resolve_target(
        &self,
        target: &FileTarget,
        write: bool,
    ) -> Result<ResolvedFileTarget, ApiError>;

    fn with_write_lease<T>(
        &self,
        target: &FileTarget,
        expected_root: &Path,
        operation: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError>;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MutationKey {
    parent: Identity,
    name: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct LocalFileService {
    roots: Mutex<HashMap<String, (PathBuf, Identity)>>,
    mutations: Mutex<HashMap<MutationKey, Weak<Mutex<()>>>>,
}

impl LocalFileService {
    pub(crate) fn dispatch(
        &self,
        method: &str,
        payload: Value,
        access: &impl FileTargetAccess,
    ) -> Result<Value, ApiError> {
        match method {
            method::FILE_LIST => {
                let request: FileListRequest = decode(payload)?;
                encode(listing::list(self, &request, access)?)
            }
            method::FILE_READ => {
                let request: FileReadRequest = decode(payload)?;
                encode(self.read(&request, access)?)
            }
            method::FILE_WRITE => {
                let request: FileWriteRequest = decode(payload)?;
                encode(self.write(&request, access)?)
            }
            _ => Err(api(
                "method_not_found",
                "The requested file operation is unknown.",
            )),
        }
    }

    /// Shared safe reader for explicitly selected, registered S3 capabilities.
    pub(crate) fn read(
        &self,
        request: &FileReadRequest,
        access: &impl FileTargetAccess,
    ) -> Result<FileReadResponse, ApiError> {
        let resolved = access.resolve_target(&request.target, false)?;
        let (root, root_identity) = self.open_target(&request.target, &resolved)?;
        let (parent_bytes, name) = split_file_path(&request.path_base64)?;
        let parent = open_directory(&root, parent_bytes)?;
        let parent_identity = identity(&fstat(&parent).map_err(io_error)?);
        let observed = read_leaf(&parent, &name)?;
        let current = access.resolve_target(&request.target, false)?;
        if current.root != resolved.root {
            return Err(target_changed());
        }
        revalidate_namespace(&resolved.root, root_identity, parent_bytes, parent_identity)?;
        Ok(FileReadResponse {
            path_base64: request.path_base64.clone(),
            size_bytes: u64::try_from(observed.text.len()).map_err(|_| invalid_input())?,
            modified_at_ms: observed.fingerprint.modified_at_ms(),
            text: observed.text,
            revision: observed.revision,
            observed_at_ms: now_ms()?,
        })
    }

    pub(crate) fn write(
        &self,
        request: &FileWriteRequest,
        access: &impl FileTargetAccess,
    ) -> Result<FileWriteResponse, ApiError> {
        write::save(self, request, access, &write::WriteFaults::default())
    }

    fn open_target(
        &self,
        target: &FileTarget,
        resolved: &ResolvedFileTarget,
    ) -> Result<(std::os::fd::OwnedFd, Identity), ApiError> {
        let root = open_root(&resolved.root)?;
        let observed = identity(&fstat(&root).map_err(io_error)?);
        let mut roots = self.roots.lock().map_err(|_| {
            api(
                "file_io_error",
                "The file service state could not be locked.",
            )
        })?;
        let key = format!("{target:?}");
        if let Some((previous_path, previous_identity)) = roots.get(&key) {
            if *previous_identity != observed || *previous_path != resolved.root {
                return Err(target_changed());
            }
        } else {
            roots.insert(key, (resolved.root.clone(), observed));
        }
        Ok((root, observed))
    }

    fn mutation(&self, key: MutationKey) -> Result<Arc<Mutex<()>>, ApiError> {
        let mut mutations = self.mutations.lock().map_err(|_| {
            api(
                "file_io_error",
                "The file mutation registry could not be locked.",
            )
        })?;
        mutations.retain(|_, lock| lock.strong_count() != 0);
        if let Some(lock) = mutations.get(&key).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(Mutex::new(()));
        mutations.insert(key, Arc::downgrade(&lock));
        Ok(lock)
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|_| invalid_input())
}

fn encode(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| api("file_io_error", "The file response could not be encoded."))
}

fn now_ms() -> Result<i64, ApiError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .ok_or_else(|| {
            api(
                "file_io_error",
                "The current file-operation timestamp is unavailable.",
            )
        })
}
