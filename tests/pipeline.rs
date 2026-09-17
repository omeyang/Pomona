mod common;
use std::process::Command;

use common::*;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_git-pomona"))
}

fn run_ok(c: &mut Command) -> String {
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn analyze_clean_push_end_to_end() {
    let f = release_fixture();
    let plan = f.dir.path().join("plan.toml");
    let work = f.dir.path().join("work.git");
    let tips: Vec<String> = ["release-1", "release-2", "release-3"]
        .iter()
        .map(|b| blob_of(&f.upstream, b, "app.tar.gz"))
        .collect();
    let hist: Vec<String> = history_blobs(&f.upstream, "app.tar.gz")
        .into_iter()
        .filter(|o| !tips.contains(o))
        .collect();
    assert_eq!(hist.len(), 6);

    // ① analyze（只给本地目录）
    let out = run_ok(bin().args([
        "analyze",
        f.dev.to_str().unwrap(),
        "--auto-detect",
        "--min-size",
        "100000",
        "-o",
        plan.to_str().unwrap(),
    ]));
    assert!(out.contains("app.tar.gz"), "{out}");
    let text = std::fs::read_to_string(&plan).unwrap();
    assert!(
        text.contains(f.upstream.to_str().unwrap()),
        "plan 应含从 config 读到的 remote URL"
    );
    assert!(text.contains("path = \"app.tar.gz\""));
    assert_eq!(
        git(&f.dev, &["status", "--porcelain"]),
        "",
        "analyze 不动本地克隆"
    );

    // ② clean
    let out = run_ok(bin().args([
        "clean",
        plan.to_str().unwrap(),
        "--workdir",
        work.to_str().unwrap(),
        "--assume-yes",
    ]));
    assert!(out.contains("清理完成"), "{out}");
    let heads = git(
        &work,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    );
    assert_eq!(
        heads.lines().collect::<Vec<_>>(),
        ["main", "release-1", "release-2", "release-3"]
    );
    for (b, t) in ["release-1", "release-2", "release-3"].iter().zip(&tips) {
        assert_eq!(&blob_of(&work, b, "app.tar.gz"), t, "{b} tip 包保留");
    }
    for h in &hist {
        assert!(!object_exists(&work, h), "镜像里历史包 {h} 应已删");
    }
    assert!(f.dir.path().join("work.git.pomona-push.toml").is_file());

    // ③ push dry-run 不改远程
    let before = git(&f.upstream, &["rev-parse", "release-1"]);
    let out = run_ok(bin().args(["push", work.to_str().unwrap()]));
    assert!(out.contains("push --force"), "{out}");
    assert!(out.contains("tip 树与清理前一致"), "{out}");
    assert_eq!(git(&f.upstream, &["rev-parse", "release-1"]), before);

    // ③ push --yes
    run_ok(bin().args(["push", work.to_str().unwrap(), "--yes"]));
    let fresh = f.dir.path().join("fresh");
    git(
        f.dir.path(),
        &[
            "clone",
            "-q",
            "--no-local",
            f.upstream.to_str().unwrap(),
            fresh.to_str().unwrap(),
        ],
    );
    assert_eq!(blob_of(&fresh, "origin/release-1", "app.tar.gz"), tips[0]);
    for h in &hist {
        assert!(!object_exists(&fresh, h), "新克隆不应再有历史包");
    }
}

#[test]
fn analyze_refuses_url_and_missing_dir() {
    let out = bin()
        .args(["analyze", "git@github.com:x/y.git", "--auto-detect"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("本地仓库目录"));
    let out = bin()
        .args(["analyze", "/nonexistent/dir", "--auto-detect"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn interactive_review_reads_stdin() {
    use std::io::Write;
    let f = release_fixture();
    let plan = f.dir.path().join("plan.toml");
    let mut child = bin()
        .args([
            "analyze",
            f.dev.to_str().unwrap(),
            "--min-size",
            "100000",
            "-n",
            "2",
            "-o",
            plan.to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\nq\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&plan).unwrap();
    assert!(
        text.contains("path = \"app.tar.gz\"") && text.contains("reason = \"manual\""),
        "{text}"
    );
}

#[test]
fn push_runs_hooks_and_aborts_on_failing_pre_push() {
    use std::os::unix::fs::PermissionsExt;
    let f = release_fixture();
    let plan = f.dir.path().join("plan.toml");
    let work = f.dir.path().join("work.git");
    run_ok(bin().args([
        "analyze",
        f.dev.to_str().unwrap(),
        "--auto-detect",
        "--min-size",
        "100000",
        "-o",
        plan.to_str().unwrap(),
    ]));
    run_ok(bin().args([
        "clean",
        plan.to_str().unwrap(),
        "--workdir",
        work.to_str().unwrap(),
        "--assume-yes",
    ]));
    let hooks = f.dir.path().join("hooks");
    std::fs::create_dir(&hooks).unwrap();
    let pre = hooks.join("pre-push");
    std::fs::write(&pre, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&pre, std::fs::Permissions::from_mode(0o755)).unwrap();
    let before = git(&f.upstream, &["rev-parse", "release-1"]);
    let out = bin()
        .args([
            "push",
            work.to_str().unwrap(),
            "--yes",
            "--hooks-dir",
            hooks.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(
        git(&f.upstream, &["rev-parse", "release-1"]),
        before,
        "pre-push 失败不应推送"
    );

    let marker = f.dir.path().join("post-marker");
    std::fs::write(&pre, "#!/bin/sh\nexit 0\n").unwrap();
    let post = hooks.join("post-push");
    std::fs::write(
        &post,
        format!(
            "#!/bin/sh\necho \"$POMONA_BRANCHES $POMONA_REMOVED_BLOBS\" > {}\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&post, std::fs::Permissions::from_mode(0o755)).unwrap();
    run_ok(bin().args([
        "push",
        work.to_str().unwrap(),
        "--yes",
        "--hooks-dir",
        hooks.to_str().unwrap(),
    ]));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap().trim(),
        "main,release-1,release-2,release-3 6"
    );
    assert_ne!(git(&f.upstream, &["rev-parse", "release-1"]), before);
}
