/*
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

//! The real binary, asked who it is: `--version` names the version, the build stamp over its
//! sources, and the wire's protocol, and exits well - so a log's `build` line can always be
//! matched to a binary by hand. Adopted from the owner's `project_nimrod`.

use std::process::Command;

#[test]
fn version_names_the_build_stamp_and_exits_well() {
    let output = Command::new(env!("CARGO_BIN_EXE_master-control"))
        .arg("--version")
        .output()
        .expect("the binary runs");
    assert!(output.status.success(), "--version exits 0: {output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    let stamp = master_control::build_info::build_hash_hex();
    assert_eq!(stamp.len(), 16, "sixteen hex digits");
    assert!(
        text.contains(&format!("build={stamp}")),
        "the binary names the same stamp the library was built with: {text}"
    );
    assert!(text.contains("Link protocol"), "{text}");
    // The count is printed beside the stamp; the binary and the library agree on it.
    assert!(
        text.contains(&format!(
            "({} source files)",
            master_control::build_info::BUILD_FILES_HASHED
        )),
        "{text}"
    );
}
