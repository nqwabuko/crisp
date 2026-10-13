//! ink carries the library sources as an unmodified copy, mounted as a module
//! inside its own crate (see docs/rust.md). There, `crate::` means ink, so a
//! library file that names the crate root compiles here and breaks there.
//! Library files reach each other with `super::`, which means the same thing in
//! both places. `main.rs` is the binary, never copied, so it is exempt.

use std::fs;
use std::path::Path;

#[test]
fn library_files_never_name_the_crate_root() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut checked = 0;
    for entry in fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || name == "main.rs" {
            continue;
        }
        checked += 1;
        for (i, line) in fs::read_to_string(&path).unwrap().lines().enumerate() {
            if line.contains("crate::") {
                offenders.push(format!("src/{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        checked >= 6,
        "expected the library files under src/, found {checked}"
    );
    assert!(
        offenders.is_empty(),
        "use super:: instead, so ink's copy compiles:\n{}",
        offenders.join("\n")
    );
}
