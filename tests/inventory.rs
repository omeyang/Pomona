mod common;
use common::*;
use pomona::git::ProcessGit;
use pomona::inventory::{KeepRefs, build, expand_globs};
use pomona::model::SortBy;

#[test]
fn inventory_from_dev_clone() {
    let f = release_fixture();
    let g = ProcessGit;
    let branch_refs = expand_globs(&g, &f.dev, &["refs/remotes/origin/".to_string()]).unwrap();
    assert_eq!(branch_refs.len(), 4, "{branch_refs:?}");
    let keep = KeepRefs {
        branch_refs,
        branch_prefix: "refs/remotes/origin/".into(),
        extra_refs: vec![],
    };
    let inv = build(&g, &f.dev, &keep, SortBy::Max).unwrap();
    let app = inv.stats.iter().find(|s| s.path == "app.tar.gz").unwrap();
    assert_eq!(app.versions, 9);
    assert_eq!(
        inv.tip_branches("app.tar.gz"),
        vec!["release-1", "release-2", "release-3"]
    );
    assert_eq!(inv.tip_branches("a.go").len(), 4);
    assert_eq!(inv.keep_blobs.len(), 4, "a.go + 3 个 tip 包");
    assert!(!inv.is_orphan("app.tar.gz"));
    assert_eq!(inv.stats[0].path, "app.tar.gz");
}
