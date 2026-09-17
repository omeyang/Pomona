#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub upstream: PathBuf,
    pub dev: PathBuf,
}

pub fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {:?} 失败: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// 随机 base64 文本：不可压缩，且是合法文本
pub fn write_random(path: &Path, bytes: usize) {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut buf)
        .unwrap();
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let enc: Vec<u8> = buf
        .chunks(3)
        .flat_map(|c| {
            let n = (c[0] as u32) << 16
                | (*c.get(1).unwrap_or(&0) as u32) << 8
                | *c.get(2).unwrap_or(&0) as u32;
            (0..4).map(move |i| T[((n >> (18 - 6 * i)) & 63) as usize])
        })
        .collect();
    std::fs::write(path, enc).unwrap();
}

pub fn seed_repo(dir: &Path) -> PathBuf {
    let seed = dir.join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    git(&seed, &["config", "user.email", "t@t.io"]);
    git(&seed, &["config", "user.name", "t"]);
    seed
}

/// bare 远程: main(a.go) + release-1..3 各 3 版 app.tar.gz(~400KB)；dev = 普通克隆（只检出 main）
pub fn release_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let upstream = dir.path().join("upstream.git");
    git(
        dir.path(),
        &["init", "-q", "--bare", upstream.to_str().unwrap()],
    );
    let seed = seed_repo(dir.path());
    std::fs::write(seed.join("a.go"), "package main\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "base"]);
    git(
        &seed,
        &["remote", "add", "origin", upstream.to_str().unwrap()],
    );
    git(&seed, &["push", "-q", "origin", "main"]);
    for b in ["release-1", "release-2", "release-3"] {
        git(&seed, &["checkout", "-q", "-b", b, "main"]);
        for v in 1..=3 {
            write_random(&seed.join("app.tar.gz"), 300_000);
            git(&seed, &["add", "-A"]);
            git(&seed, &["commit", "-qm", &format!("{b} v{v}")]);
        }
        git(&seed, &["push", "-q", "origin", b]);
    }
    let dev = dir.path().join("dev");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            dev.to_str().unwrap(),
        ],
    );
    Fixture { dir, upstream, dev }
}

pub fn blob_of(repo: &Path, rev: &str, path: &str) -> String {
    git(repo, &["rev-parse", &format!("{rev}:{path}")])
}

pub fn history_blobs(repo: &Path, path: &str) -> Vec<String> {
    let out = git(repo, &["rev-list", "--objects", "--all"]);
    let mut v: Vec<String> = out
        .lines()
        .filter_map(|l| {
            let (oid, p) = l.split_once(' ')?;
            (p == path).then(|| oid.to_string())
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn object_exists(repo: &Path, oid: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e", oid])
        .status()
        .unwrap()
        .success()
}
