//! The corpus the test suites run over.

// Each test binary compiles its own copy of this module and uses a subset of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The recorded contract: `fixtures/*.md` and `fixtures/golden.json`.
pub fn fixtures_dir() -> PathBuf {
    manifest().join("fixtures")
}

/// The documents to run the engine over.
///
/// Real prose first. Two of crisp's three segmentation bugs survived a
/// seven-document corpus of purpose-built fixtures and were only caught by
/// genuinely messy writing, so the skill's own documentation is in here on
/// purpose: tables, links, bold sentences, inline code mid-sentence and hard
/// wraps, none of it arranged to be convenient.
pub fn corpus() -> Vec<PathBuf> {
    let mut paths = vec![manifest().join("README.md"), manifest().join("SKILL.md")];
    paths.extend(markdown_in(manifest().join("references")));
    paths.extend(markdown_in(fixtures_dir()));
    paths.retain(|p| p.is_file());
    assert!(
        paths.len() >= 12,
        "the corpus holds only {} documents",
        paths.len()
    );
    paths
}

/// The recorded fixtures alone, sorted: what `golden.json` holds an entry for.
pub fn fixture_paths() -> Vec<PathBuf> {
    markdown_in(fixtures_dir())
}

fn markdown_in(dir: PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    paths.sort();
    paths
}

pub fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}
