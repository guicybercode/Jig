#![allow(dead_code)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use cli_master_agents::{LaunchContext, LaunchEnvironment};
use tempfile::TempDir;

pub fn executable(directory: &Path, name: &str) -> PathBuf {
    script(directory, name, "echo ok")
        .canonicalize()
        .expect("fixture executable should canonicalize")
}

pub fn script(directory: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(directory).expect("fixture directory should be created");
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("fixture executable should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fixture metadata should exist")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("fixture should become executable");
    path
}

pub fn context(temp: &TempDir) -> LaunchContext {
    LaunchContext::new(
        temp.path(),
        LaunchEnvironment::from_search_paths([temp.path()]),
    )
}

pub fn isolated_env(temp: &TempDir) -> LaunchEnvironment {
    LaunchEnvironment::from_search_paths([temp.path()])
}
