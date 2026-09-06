use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;

use cli_master_core::ApiError;
use cli_master_core::wire::{FileWriteRequest, FileWriteResponse, MAX_FILE_TEXT_BYTES};
use rustix::fs::{AtFlags, Mode, OFlags, fstat, fsync, openat, renameat, statat, unlinkat};

use super::descriptor::{
    Fingerprint, identity, open_directory, read_leaf, revalidate_namespace, split_file_path,
};
use super::error::{api, conflict, invalid_input, io_error, target_changed};
use super::{FileTargetAccess, LocalFileService, MutationKey, attributes, metadata, now_ms};

#[derive(Default)]
pub(super) struct WriteFaults {
    #[cfg(test)]
    pub after_staging: Option<Box<dyn Fn()>>,
    #[cfg(test)]
    pub fail_after_rename: bool,
}

#[allow(
    clippy::unused_self,
    reason = "fault hooks are inert outside deterministic tests"
)]
impl WriteFaults {
    fn after_staging(&self) {
        #[cfg(test)]
        if let Some(hook) = &self.after_staging {
            hook();
        }
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "tests inject an error after publication to verify durability reporting"
    )]
    fn after_rename(&self) -> Result<(), ApiError> {
        #[cfg(test)]
        if self.fail_after_rename {
            return Err(api(
                "file_io_error",
                "Injected directory durability failure.",
            ));
        }
        Ok(())
    }
}

pub(super) fn save(
    service: &LocalFileService,
    request: &FileWriteRequest,
    access: &impl FileTargetAccess,
    faults: &WriteFaults,
) -> Result<FileWriteResponse, ApiError> {
    if request.text.len() > MAX_FILE_TEXT_BYTES || request.text.contains('\0') {
        return Err(invalid_input());
    }
    let resolved = access.resolve_target(&request.target, true)?;
    let (root, root_identity) = service.open_target(&request.target, &resolved)?;
    let (parent_bytes, name) = split_file_path(&request.path_base64)?;
    let parent = open_directory(&root, parent_bytes)?;
    let parent_identity = identity(&fstat(&parent).map_err(io_error)?);
    let lock = service.mutation(MutationKey {
        parent: parent_identity,
        name: name.as_bytes().to_vec(),
    })?;
    let _mutation = lock
        .lock()
        .map_err(|_| api("file_io_error", "The file mutation lock is unavailable."))?;
    let original = read_leaf(&parent, &name)?;
    if original.revision != request.expected_revision {
        return Err(conflict(Some(original.revision.as_str())));
    }
    metadata::inspect(&original.file, &original.fingerprint)?;
    let mut temporary = TemporaryFile::new(&parent)?;
    temporary
        .file
        .write_all(request.text.as_bytes())
        .map_err(io_error)?;
    temporary.file.flush().map_err(io_error)?;
    metadata::apply(&original.file, &temporary.file, &original.fingerprint)?;
    fsync(&temporary.file).map_err(io_error)?;
    let staged_fingerprint = Fingerprint::from_stat(&fstat(&temporary.file).map_err(io_error)?);
    faults.after_staging();

    access.with_write_lease(&request.target, &resolved.root, || {
        let current = read_leaf(&parent, &name).map_err(|error| match error.code.as_str() {
            "file_not_found"
            | "file_not_regular"
            | "file_symlink_not_allowed"
            | "file_too_large"
            | "file_not_text" => conflict(None),
            _ => error,
        })?;
        if current.revision != request.expected_revision {
            return Err(conflict(Some(current.revision.as_str())));
        }
        metadata::inspect(&current.file, &current.fingerprint)?;
        temporary.validate_ownership()?;
        let staged_now = Fingerprint::from_stat(&fstat(&temporary.file).map_err(io_error)?);
        if staged_now != staged_fingerprint {
            return Err(conflict(None));
        }
        metadata::inspect(&temporary.file, &staged_now)?;
        attributes::verify_preserved(&current.file, &temporary.file)?;
        revalidate_namespace(&resolved.root, root_identity, parent_bytes, parent_identity)?;
        renameat(&parent, &temporary.name, &parent, &name).map_err(io_error)?;
        temporary.published = true;

        let published = Fingerprint::from_stat(
            &fstat(&temporary.file).map_err(|_| durability_uncertain(None))?,
        );
        let revision = published
            .revision(request.text.as_bytes())
            .map_err(|_| durability_uncertain(None))?;
        faults
            .after_rename()
            .map_err(|_| durability_uncertain(Some(revision.as_str())))?;
        fsync(&parent).map_err(|_| durability_uncertain(Some(revision.as_str())))?;
        let namespace = statat(&parent, &name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| durability_uncertain(Some(revision.as_str())))?;
        let observed = Fingerprint::from_stat(&namespace);
        if observed != published {
            return Err(durability_uncertain(Some(revision.as_str())));
        }
        Ok(FileWriteResponse {
            path_base64: request.path_base64.clone(),
            revision,
            size_bytes: u64::try_from(request.text.len())
                .map_err(|_| durability_uncertain(None))?,
            modified_at_ms: published.modified_at_ms(),
            written_at_ms: now_ms().map_err(|_| durability_uncertain(None))?,
        })
    })
}

fn durability_uncertain(revision: Option<&str>) -> ApiError {
    let mut error = ApiError::new(
        "file_durability_uncertain",
        "The replacement was applied, but its final durability could not be confirmed.",
    )
    .with_action(
        "Keep the draft and re-read the file before retrying; the write may already be present.",
    )
    .with_detail("writeApplied", true);
    if let Some(revision) = revision {
        error = error.with_detail("currentRevision", revision);
    }
    error
}

struct TemporaryFile<'a> {
    parent: &'a OwnedFd,
    name: OsString,
    file: File,
    identity: super::descriptor::Identity,
    published: bool,
}

impl<'a> TemporaryFile<'a> {
    fn new(parent: &'a OwnedFd) -> Result<Self, ApiError> {
        let name = OsString::from(format!(
            ".cli-master-save-{}",
            uuid::Uuid::now_v7().simple()
        ));
        let fd = openat(
            parent,
            &name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(io_error)?;
        // If identity cannot be established, leave the uniquely named temporary
        // file for manual inspection rather than unlinking an unproven name.
        let identity = identity(&fstat(&fd).map_err(io_error)?);
        let file = File::from(fd);
        Ok(Self {
            parent,
            name,
            file,
            identity,
            published: false,
        })
    }

    fn validate_ownership(&self) -> Result<(), ApiError> {
        let stat = statat(self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| target_changed())?;
        if identity(&stat) != self.identity || stat.st_nlink != 1 {
            return Err(target_changed());
        }
        Ok(())
    }
}

impl Drop for TemporaryFile<'_> {
    fn drop(&mut self) {
        if !self.published && self.validate_ownership().is_ok() {
            let _ = unlinkat(self.parent, &self.name, AtFlags::empty());
        }
    }
}
