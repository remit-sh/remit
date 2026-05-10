use std::fs;

use domain::StandingGuidanceReader;
use tempfile::TempDir;

use super::*;

fn write(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap();
}

fn touch_dir(dir: &Path, name: &str) {
    fs::create_dir(dir.join(name)).unwrap();
}

#[test]
fn discover_returns_none_when_walk_reaches_filesystem_root_with_no_remit_or_git() {
    let tmp = TempDir::new().unwrap();
    assert!(discover_remit_path(tmp.path()).unwrap().is_none());
}

#[test]
fn discover_returns_remit_path_when_present_at_cwd() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "# project");
    let path = discover_remit_path(tmp.path()).unwrap().unwrap();
    assert_eq!(path, tmp.path().join("REMIT.md"));
}

#[test]
fn discover_walks_up_to_find_remit_in_parent() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "# parent");
    let sub = tmp.path().join("crates/foo");
    fs::create_dir_all(&sub).unwrap();
    let path = discover_remit_path(&sub).unwrap().unwrap();
    assert_eq!(path, tmp.path().join("REMIT.md"));
}

#[test]
fn discover_stops_at_git_boundary_when_no_remit_below_or_at_it() {
    let tmp = TempDir::new().unwrap();
    // Project root has .git but no REMIT.md; ancestor has REMIT.md.
    let project = tmp.path().join("project");
    fs::create_dir(&project).unwrap();
    touch_dir(&project, ".git");
    write(tmp.path(), "REMIT.md", "# stray ancestor");
    let sub = project.join("crates/foo");
    fs::create_dir_all(&sub).unwrap();
    assert!(
        discover_remit_path(&sub).unwrap().is_none(),
        ".git boundary must prevent walk into ancestor REMIT.md"
    );
}

#[test]
fn discover_prefers_remit_over_git_at_same_level() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "# project");
    touch_dir(tmp.path(), ".git");
    let path = discover_remit_path(tmp.path()).unwrap().unwrap();
    assert_eq!(path, tmp.path().join("REMIT.md"));
}

#[test]
fn discover_treats_git_file_as_boundary_not_just_directory() {
    // Linked worktrees use `.git` as a *file*, not a directory.
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    fs::create_dir(&project).unwrap();
    write(&project, ".git", "gitdir: /elsewhere");
    write(tmp.path(), "REMIT.md", "# stray ancestor");
    assert!(
        discover_remit_path(&project).unwrap().is_none(),
        ".git as a file must also act as a boundary"
    );
}

#[test]
fn discover_inner_git_acts_as_boundary_for_inner_cwd() {
    // Submodule case: inner repo has its own .git; project's REMIT.md is above.
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "# project");
    touch_dir(tmp.path(), ".git");
    let submodule = tmp.path().join("vendor/sub");
    fs::create_dir_all(&submodule).unwrap();
    touch_dir(&submodule, ".git");
    assert!(
        discover_remit_path(&submodule).unwrap().is_none(),
        "inner .git boundary must hide outer REMIT.md from inner cwd"
    );
}

#[test]
fn read_returns_empty_standing_guidance_when_no_remit_found() {
    let tmp = TempDir::new().unwrap();
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let g = reader.read().unwrap();
    assert!(g.project.is_none());
    assert!(g.agent_template.is_none());
}

#[test]
fn read_returns_remit_contents_in_project_slot_when_present() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "# project\n\nbody");
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let g = reader.read().unwrap();
    assert_eq!(g.project.as_deref(), Some("# project\n\nbody"));
    assert!(
        g.agent_template.is_none(),
        "reserved slot must stay None until #32"
    );
}

#[test]
fn read_strips_leading_bom() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), "REMIT.md", "\u{FEFF}# project");
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let g = reader.read().unwrap();
    assert_eq!(g.project.as_deref(), Some("# project"));
}

#[test]
fn read_preserves_internal_whitespace() {
    let tmp = TempDir::new().unwrap();
    let body = "  leading\ttabs\n\nblank lines\n\ntrailing  ";
    write(tmp.path(), "REMIT.md", body);
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let g = reader.read().unwrap();
    assert_eq!(g.project.as_deref(), Some(body));
}

/// Walk the source chain produced by `thiserror`-style `#[source]` so tests
/// can assert the underlying cause's message, not just the outer wrapper.
fn full_chain(err: &domain::StandingGuidanceError) -> String {
    let mut out = err.to_string();
    let mut current: Option<&dyn std::error::Error> = std::error::Error::source(err);
    while let Some(c) = current {
        out.push_str(": ");
        out.push_str(&c.to_string());
        current = c.source();
    }
    out
}

#[test]
fn read_returns_unavailable_when_remit_exceeds_size_cap() {
    let tmp = TempDir::new().unwrap();
    let oversized = "x".repeat((MAX_REMIT_BYTES + 1) as usize);
    write(tmp.path(), "REMIT.md", &oversized);
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let err = reader.read().unwrap_err();
    let chain = full_chain(&err);
    assert!(
        chain.contains("REMIT.md exceeds") && chain.contains(&MAX_REMIT_BYTES.to_string()),
        "expected size-cap reason in chain, got: {chain}"
    );
}

#[test]
fn read_returns_unavailable_when_remit_is_invalid_utf8() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("REMIT.md"), [0xFF, 0xFE, 0xFD]).unwrap();
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let err = reader.read().unwrap_err();
    let chain = full_chain(&err);
    assert!(
        chain.contains("not valid UTF-8"),
        "expected UTF-8 reason in chain, got: {chain}"
    );
}

#[test]
fn read_returns_unavailable_when_remit_is_a_directory() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("REMIT.md")).unwrap();
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let err = reader.read().unwrap_err();
    let chain = full_chain(&err);
    assert!(
        chain.contains("not a regular file"),
        "expected directory reason in chain, got: {chain}"
    );
}

#[test]
fn read_returns_some_at_exactly_the_size_cap() {
    let tmp = TempDir::new().unwrap();
    let exactly = "x".repeat(MAX_REMIT_BYTES as usize);
    write(tmp.path(), "REMIT.md", &exactly);
    let reader = FileStandingGuidanceReader::new(tmp.path().to_path_buf());
    let g = reader.read().unwrap();
    assert_eq!(
        g.project.as_deref().unwrap().len(),
        MAX_REMIT_BYTES as usize
    );
}
