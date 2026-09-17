mod common;
use std::collections::BTreeMap;

use common::*;
use pomona::git::ProcessGit;
use pomona::hooks::{HookOutcome, run_hook};
use pomona::publish::{PushInfo, largest_files, push_args, snapshot_tips, verify};

#[test]
fn pushinfo_roundtrip_and_path() {
    let tmp = tempfile::tempdir().unwrap();
    let mirror = tmp.path().join("work.git");
    let p = PushInfo::path_for(&mirror);
    assert_eq!(p.file_name().unwrap(), "work.git.pomona-push.toml");
    assert_eq!(p.parent().unwrap(), tmp.path());
    let info = PushInfo {
        version: 1,
        url: Some("git@h:r.git".into()),
        branches: vec!["main".into()],
        tips: BTreeMap::from([("refs/heads/main".to_string(), "abc".to_string())]),
        removed: vec!["d1".into()],
        reclaim_bytes: 42,
    };
    info.save(&p).unwrap();
    let back = PushInfo::load(&p).unwrap();
    assert_eq!(back, info);
}

#[test]
fn verify_detects_tree_drift_and_reachable_removed() {
    let f = release_fixture();
    let g = ProcessGit;
    let refs = vec![
        "refs/heads/main".to_string(),
        "refs/heads/release-1".to_string(),
    ];
    let tips = snapshot_tips(&g, &f.upstream, &refs).unwrap();
    assert_eq!(tips.len(), 2);
    let ok = PushInfo {
        version: 1,
        url: None,
        branches: vec![],
        tips: tips.clone(),
        removed: vec!["0000000000000000000000000000000000000000".into()],
        reclaim_bytes: 0,
    };
    verify(&g, &f.upstream, &ok).unwrap();
    let mut drift = ok.clone();
    drift
        .tips
        .insert("refs/heads/main".into(), "deadbeef".into());
    assert!(verify(&g, &f.upstream, &drift).is_err());
    let mut reachable = ok.clone();
    reachable.removed = vec![blob_of(&f.upstream, "release-1", "app.tar.gz")];
    assert!(verify(&g, &f.upstream, &reachable).is_err());
}

#[test]
fn largest_files_lists_biggest_first() {
    let f = release_fixture();
    let v = largest_files(&ProcessGit, &f.upstream, "release-1", 2).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].1, "app.tar.gz");
    assert!(v[0].0 > v[1].0);
}

#[test]
fn push_args_modes() {
    assert_eq!(
        push_args("u", false),
        [
            "push",
            "--force",
            "u",
            "refs/heads/*:refs/heads/*",
            "refs/tags/*:refs/tags/*"
        ]
    );
    assert_eq!(push_args("u", true), ["push", "--mirror", "--force", "u"]);
}

#[test]
fn hooks_run_with_env_and_fail_on_nonzero() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        run_hook(tmp.path(), "pre-push", &[]).unwrap(),
        HookOutcome::Missing
    ));
    let marker = tmp.path().join("marker");
    let hook = tmp.path().join("pre-push");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\necho \"$POMONA_URL\" > {}\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        run_hook(
            tmp.path(),
            "pre-push",
            &[("POMONA_URL", "git@x".to_string())]
        )
        .unwrap(),
        HookOutcome::Ran
    ));
    assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "git@x");
    std::fs::write(&hook, "#!/bin/sh\nexit 3\n").unwrap();
    assert!(run_hook(tmp.path(), "pre-push", &[]).is_err());
}
