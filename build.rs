/*
    Master Control loads Link as the DLL beside its executable - never as a crate - so the build
    must produce that DLL and put it where the residence rule says it lives. This script builds
    the pinned submodule's cdylib with its own target directory (a nested cargo invocation
    against the outer target directory would deadlock on cargo's lock) and copies the library
    beside both the binaries and the test executables.

    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The build's own provenance: a hash over every source this binary was compiled from - the
/// crate's Rust, its manifests, this script, and the wire's sources - written into OUT_DIR as
/// `build_info.rs`. Git-independent on purpose, so a tarball or a container build stamps as
/// honestly as a checkout. `--version` prints it and the input log records it, so a replay that
/// disagrees can say "a different binary" before anyone says "a simulation bug".
///
/// Adopted from the owner's `project_nimrod`: std only, `DefaultHasher` with its fixed keys, a
/// fixed walk order, and the file count mixed in so a file lost is a change too.
fn stamp_build(manifest_dir: &Path, out_dir: &Path) {
    let mut files: Vec<PathBuf> = Vec::new();
    for root in [
        "src",
        "tests",
        "build.rs",
        "Cargo.toml",
        "Cargo.lock",
        "external/link/src",
        "external/link/Cargo.toml",
    ] {
        collect_sources(&manifest_dir.join(root), &mut files);
    }
    files.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in &files {
        let relative = file.strip_prefix(manifest_dir).unwrap_or(file);
        hasher.write(relative.to_string_lossy().replace('\\', "/").as_bytes());
        hasher.write(&std::fs::read(file).unwrap_or_default());
        println!("cargo::rerun-if-changed={}", file.display());
    }
    hasher.write_usize(files.len());
    let stamp = hasher.finish();
    let generated = format!(
        "/// A hash over every source this binary was built from; see build.rs.\npub const BUILD_HASH: u64 = {stamp:#018x};\n/// How many source files the hash covers.\npub const BUILD_FILES_HASHED: usize = {};\n",
        files.len()
    );
    let mut out =
        std::fs::File::create(out_dir.join("build_info.rs")).expect("OUT_DIR is writable");
    out.write_all(generated.as_bytes())
        .expect("build_info.rs written");
}

fn collect_sources(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        files.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry = entry.path();
        let name = entry
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "target" || name == ".git" {
            continue;
        }
        if entry.is_dir() {
            collect_sources(&entry, files);
        } else if matches!(
            entry.extension().and_then(|e| e.to_str()),
            Some("rs" | "toml" | "lock" | "h" | "txt")
        ) {
            files.push(entry);
        }
    }
}

fn main() {
    // Rebuild when the wire changes: the submodule's sources and its contract of record.
    println!("cargo::rerun-if-changed=external/link/src");
    println!("cargo::rerun-if-changed=external/link/include");
    println!("cargo::rerun-if-changed=external/link/Cargo.toml");

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));

    stamp_build(&manifest_dir, &out_dir);

    let link_manifest = manifest_dir
        .join("external")
        .join("link")
        .join("Cargo.toml");
    assert!(
        link_manifest.exists(),
        "external/link is empty - initialise submodules first: git submodule update --init"
    );

    // The wire is built release even under a debug build of this process: it is a consumed
    // artefact, the very same binary every TronGrid Lite runs beside, not a debuggee of ours.
    let link_target = out_dir.join("link-target");
    // A plain build of the wire's own crate, whatever this process is doing: under `cargo
    // clippy` the outer invocation hands clippy's driver down through RUSTC_WORKSPACE_WRAPPER
    // and friends, and the nested cargo would then lint Link - with *this* crate's clippy.toml,
    // found by walking up from external/link - against rules that are this crate's alone.
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .args(["build", "--release", "--manifest-path"])
        .arg(&link_manifest)
        .arg("--target-dir")
        .arg(&link_target)
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("CLIPPY_ARGS")
        .env_remove("CLIPPY_CONF_DIR")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("cargo must be runnable to build the wire");
    assert!(status.success(), "building the Link cdylib failed");

    let library_name = if cfg!(target_os = "windows") {
        "link.dll"
    } else {
        "liblink.so"
    };
    let built = link_target.join("release").join(library_name);
    assert!(built.exists(), "the Link build produced no {library_name}");

    /*
        OUT_DIR sits at target/<profile>/build/<pkg>-<hash>/out, so the profile directory is
        three levels up. The binary lands there; test executables land in its deps/ - the
        residence rule resolves beside the running executable, so the library is copied beside
        both.
    */
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is always at least three levels below the profile directory")
        .to_path_buf();
    copy_beside(&built, &profile_dir.join(library_name));
    copy_beside(&built, &profile_dir.join("deps").join(library_name));
}

fn copy_beside(from: &Path, to: &Path) {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).expect("the target directory exists or can be made");
    }
    std::fs::copy(from, to).unwrap_or_else(|error| {
        panic!(
            "copying {} beside the executables failed: {error}",
            from.display()
        )
    });
}
