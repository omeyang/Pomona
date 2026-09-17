mod common;
use std::process::Command;

use common::*;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-pomona"))
}

/// branchA: 无扩展名包 bundle v1 -> v2 -> v3==v1（tip 回到 v1 内容）→ 按大小识别，且 tip blob == 历史 v1 blob，绝不能删
/// branchB: data.bz2 小文件三版本 → 按扩展名识别
/// main:    src.go 源码 → 必须保留
#[test]
fn same_content_tip_blob_is_never_removed() {
    let dir = tempfile::tempdir().unwrap();
    let up = dir.path().join("up.git");
    git(dir.path(), &["init", "-q", "--bare", up.to_str().unwrap()]);
    let seed = seed_repo(dir.path());
    std::fs::write(seed.join("src.go"), "package main\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "base"]);
    git(&seed, &["remote", "add", "origin", up.to_str().unwrap()]);
    git(&seed, &["push", "-q", "origin", "main"]);
    git(&seed, &["checkout", "-q", "-b", "branchA"]);
    write_random(&seed.join("bundle"), 300_000);
    let v1 = std::fs::read(seed.join("bundle")).unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "A1"]);
    write_random(&seed.join("bundle"), 300_000);
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "A2"]);
    std::fs::write(seed.join("bundle"), &v1).unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "A3=A1"]);
    git(&seed, &["push", "-q", "origin", "branchA"]);
    git(&seed, &["checkout", "-q", "main"]);
    git(&seed, &["checkout", "-q", "-b", "branchB"]);
    for v in 1..=3 {
        write_random(&seed.join("data.bz2"), 1500);
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-qm", &format!("B{v}")]);
    }
    git(&seed, &["push", "-q", "origin", "branchB"]);
    let dev = dir.path().join("dev");
    git(
        dir.path(),
        &["clone", "-q", up.to_str().unwrap(), dev.to_str().unwrap()],
    );

    let tip_a = blob_of(&up, "branchA", "bundle");
    let tip_b = blob_of(&up, "branchB", "data.bz2");
    let src = blob_of(&up, "main", "src.go");
    let v2: Vec<String> = history_blobs(&up, "bundle")
        .into_iter()
        .filter(|o| o != &tip_a)
        .collect();
    assert_eq!(v2.len(), 1);

    let plan = dir.path().join("plan.toml");
    let work = dir.path().join("work.git");
    let out = bin()
        .args([
            "analyze",
            dev.to_str().unwrap(),
            "--auto-detect",
            "--min-size",
            "100000",
            "-o",
            plan.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&plan).unwrap();
    assert!(
        text.contains("path = \"bundle\"")
            && text.contains("path = \"data.bz2\"")
            && !text.contains("src.go"),
        "{text}"
    );
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

    assert!(
        object_exists(&work, &tip_a),
        "branchA tip(=历史V1, 无扩展名) 必须保留"
    );
    assert!(object_exists(&work, &tip_b));
    assert!(object_exists(&work, &src));
    assert!(!object_exists(&work, &v2[0]), "branchA 历史 V2 应删");
    assert_eq!(blob_of(&work, "branchA", "bundle"), tip_a);
}
