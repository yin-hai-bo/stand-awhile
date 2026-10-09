use std::{env, fs, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=assets/app.ico");
    watch_git_head();
    embed_pet_frames();

    if cfg!(target_os = "windows") {
        write_version_header();
        embed_resource::compile("app.rc", embed_resource::NONE)
            .manifest_required()
            .expect("could not compile required Windows resources");
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

fn write_version_header() {
    // VERSIONINFO stores each numeric component in a 16-bit word.
    let version_word = |name: &str| {
        env::var(name)
            .expect("Cargo package version is required")
            .parse::<u16>()
            .expect("Windows version components must fit in 16 bits")
    };
    let major = version_word("CARGO_PKG_VERSION_MAJOR");
    let minor = version_word("CARGO_PKG_VERSION_MINOR");
    let patch = version_word("CARGO_PKG_VERSION_PATCH");
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo package version is required");
    let header =
        format!("#define APP_VERSION_WORDS {major},{minor},{patch},0\n#define APP_VERSION_STRING \"{version}\"\n");
    // embed-resource adds OUT_DIR to the resource compiler's include path.
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("app_version.h"), header)
        .expect("could not write Windows version definitions");
}

fn embed_pet_frames() {
    fn collect_pngs(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(directory).expect("could not read pet assets") {
            let path = entry.expect("could not read pet asset entry").path();
            if path.is_dir() {
                collect_pngs(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "png") {
                files.push(path);
            }
        }
    }

    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/pets/cat-dog");
    let mut files = Vec::new();
    collect_pngs(&root.join("png"), &mut files);
    files.sort();
    let mut source = String::from("static EMBEDDED_FRAMES: &[(&str, &[u8])] = &[\n");
    for path in files {
        let name = path.strip_prefix(&root).unwrap().to_str().unwrap().replace('\\', "/");
        source.push_str(&format!(
            "    ({name:?}, include_bytes!({:?})),\n",
            path.to_str().unwrap()
        ));
    }
    source.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("pet_frames.rs"), source)
        .expect("could not write embedded pet assets");
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
