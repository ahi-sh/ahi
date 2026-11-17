use super::*;
use std::fs;
use std::os::unix::fs::symlink;

#[test]
fn discovered_names_cannot_be_replaced_by_file_or_ancestor_symlinks() {
    let root = crate::fixture_directory::create_unique_fixture_directory("search-open-races");
    let outside = crate::fixture_directory::create_unique_fixture_directory("search-open-outside");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/data.txt"), "needle inside\n").unwrap();
    fs::write(outside.join("data.txt"), "needle SECRET\n").unwrap();
    fs::create_dir(root.join("state")).unwrap();
    fs::write(root.join("state/secret.txt"), "needle STATE\n").unwrap();
    let workspace = Workspace::open(
        &root,
        &crate::directories::Directories::single_root(root.join("state")),
    )
    .unwrap();
    let tree = walk::walk(&workspace, workspace.root(), "").unwrap();
    let candidate = tree
        .files()
        .find(|name| *name == "nested/data.txt")
        .unwrap();
    let regex = Regex::new("needle").unwrap();
    let budget = AtomicU64::new(SCAN_BUDGET);
    let mut scanner = Scanner::new(&workspace, &regex, &budget);
    match scanner.scan(candidate) {
        FileResult::Hits(hits) => assert_eq!(hits.rendered, "nested/data.txt:1:needle inside"),
        _ => panic!("ordinary descendant must be readable"),
    }
    // Deterministic replacement after enumeration, before the scanner opens.
    fs::remove_file(root.join(candidate)).unwrap();
    symlink(outside.join("data.txt"), root.join(candidate)).unwrap();
    assert!(matches!(scanner.scan(candidate), FileResult::Skipped));
    fs::remove_file(root.join(candidate)).unwrap();
    fs::remove_dir(root.join("nested")).unwrap();
    symlink(&outside, root.join("nested")).unwrap();
    assert!(matches!(scanner.scan(candidate), FileResult::Skipped));
    assert!(matches!(
        scanner.scan("state/secret.txt"),
        FileResult::Skipped
    ));
    fs::remove_file(root.join("nested")).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join(candidate), "needle restored\n").unwrap();
    match scanner.scan(candidate) {
        FileResult::Hits(hits) => assert_eq!(hits.rendered, "nested/data.txt:1:needle restored"),
        _ => panic!("failed opens must not poison subsequent reads"),
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn anchored_reads_survive_root_path_replacement_and_reject_traversal() {
    let base = crate::fixture_directory::create_unique_fixture_directory("search-root-race");
    let root = base.join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("data.txt"), "original").unwrap();
    let directory = ahi_unix::directory::Directory::open(&root).unwrap();
    fs::rename(&root, base.join("moved")).unwrap();
    fs::create_dir(&root).unwrap();
    fs::write(root.join("data.txt"), "replacement").unwrap();
    let mut text = String::new();
    directory
        .open_file(Path::new("data.txt"))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "original");
    for path in [
        "",
        "..",
        "../workspace/data.txt",
        "/etc/passwd",
        "data.txt/child",
        "bad\0name",
    ] {
        assert!(directory.open_file(Path::new(path)).is_err(), "{path:?}");
    }
    assert!(directory.open_file(Path::new(".")).is_err());
    fs::remove_dir_all(base).unwrap();
}
