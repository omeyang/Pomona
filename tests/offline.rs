mod common;
use std::process::Command;

use common::*;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-pomona"))
}

#[test]
fn clean_works_offline_with_local_only_plan() {
    let f = release_fixture();
    let plan = f.dir.path().join("plan.toml");
    let work = f.dir.path().join("work.git");
    let out = bin()
        .args([
            "analyze",
            f.dev.to_str().unwrap(),
            "--auto-detect",
            "--min-size",
            "100000",
            "-o",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    // 抹掉 url，模拟离线
    let text = std::fs::read_to_string(&plan).unwrap();
    let text: String = text
        .lines()
        .filter(|l| !l.starts_with("url = "))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&plan, text).unwrap();
    let out = bin()
        .args([
            "clean",
            plan.to_str().unwrap(),
            "--workdir",
            work.to_str().unwrap(),
            "--assume-yes",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("离线"));
    let heads = git(
        &work,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    );
    assert_eq!(heads.lines().count(), 4);
    assert_eq!(
        blob_of(&work, "release-3", "app.tar.gz"),
        blob_of(&f.upstream, "release-3", "app.tar.gz")
    );
    // 离线 plan 无 URL：push 需要 --url
    let out = bin()
        .args(["push", work.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let out = bin()
        .args([
            "push",
            work.to_str().unwrap(),
            "--url",
            f.upstream.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
