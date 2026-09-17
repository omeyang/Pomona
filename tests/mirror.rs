mod common;
use common::*;
use pomona::git::ProcessGit;
use pomona::mirror::{Source, build};

fn heads(repo: &std::path::Path) -> Vec<String> {
    git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

#[test]
fn online_uses_reference_and_gets_all_heads() {
    let f = release_fixture();
    let w = f.dir.path().join("m1.git");
    let src = Source::resolve(Some(f.upstream.to_str().unwrap()), Some(&f.dev)).unwrap();
    assert!(matches!(src, Source::Online { .. }));
    let b = build(&ProcessGit, &src, &w).unwrap();
    assert_eq!(b, vec!["main", "release-1", "release-2", "release-3"]);
    assert!(
        !w.join("objects/info/alternates").exists(),
        "--dissociate 后不应依赖本地对象"
    );
    assert!(
        build(&ProcessGit, &src, &w).is_err(),
        "workdir 已存在应报错"
    );
}

#[test]
fn offline_remaps_remote_tracking_refs_to_heads() {
    let f = release_fixture();
    let w = f.dir.path().join("m2.git");
    let src = Source::resolve(None, Some(&f.dev)).unwrap();
    assert!(matches!(src, Source::Offline { .. }));
    let b = build(&ProcessGit, &src, &w).unwrap();
    assert_eq!(b, vec!["main", "release-1", "release-2", "release-3"]);
    assert_eq!(heads(&w), b);
    assert_eq!(git(&w, &["for-each-ref", "refs/remotes/"]), "");
    assert_eq!(
        blob_of(&w, "release-2", "app.tar.gz"),
        blob_of(&f.upstream, "release-2", "app.tar.gz")
    );
}

#[test]
fn resolve_rejects_nothing() {
    assert!(Source::resolve(None, None).is_err());
    assert!(Source::resolve(None, Some(std::path::Path::new("/nonexistent/x"))).is_err());
    assert!(matches!(
        Source::resolve(
            Some("git@h:r.git"),
            Some(std::path::Path::new("/nonexistent/x"))
        )
        .unwrap(),
        Source::UrlOnly { .. }
    ));
}
