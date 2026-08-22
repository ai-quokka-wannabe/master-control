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

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Rebuild when the wire changes: the submodule's sources and its contract of record.
    println!("cargo::rerun-if-changed=external/link/src");
    println!("cargo::rerun-if-changed=external/link/include");
    println!("cargo::rerun-if-changed=external/link/Cargo.toml");

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));

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
