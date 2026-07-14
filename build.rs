use std::{env, fs, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=assets/app.ico");
    watch_git_head();

    if cfg!(target_os = "windows") {
        let _ = embed_resource::compile("app.rc", embed_resource::NONE);
    }

    let has_git = git_is_available() && git_work_tree().is_some();
    let commit = if has_git {
        current_commit().unwrap_or_else(|| "unknown".to_owned())
    } else {
        "unknown".to_owned()
    };

    if has_git && env::var("PROFILE").as_deref() == Ok("release") && workspace_is_dirty() {
        panic!("release build requires a clean Git workspace; commit or stash your changes before building");
    }

    println!("cargo:rustc-env=BUILD_COMMIT={commit}");
}

fn git_is_available() -> bool {
    Command::new("git")
        .args(["--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git_work_tree() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!path.is_empty()).then_some(path)
}

fn current_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!commit.is_empty()).then_some(commit)
}

fn workspace_is_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn watch_git_head() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    let Ok(head) = fs::read_to_string(".git/HEAD") else {
        return;
    };

    if let Some(ref_path) = head.strip_prefix("ref: ").map(str::trim) {
        println!("cargo:rerun-if-changed=.git/{ref_path}");
    }
}
