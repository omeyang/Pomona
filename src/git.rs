use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, anyhow};

use crate::model::{ObjectRec, Oid};

/// 对 git 子进程的最小抽象：捕获输出 / 直通终端。其余操作都是建立在这两个方法之上的辅助函数。
pub trait Git {
    /// 捕获 stdout；非 0 退出报错并附 stderr 尾部
    fn run(&self, repo: Option<&Path>, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>>;
    /// 输出直通终端（clone / push 这类有进度条的命令）
    fn run_inherit(&self, repo: Option<&Path>, args: &[&str]) -> Result<()>;
}

pub struct ProcessGit;

fn command(repo: Option<&Path>, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    if let Some(r) = repo {
        c.arg("-C").arg(r);
    }
    c.args(args);
    c
}

fn describe(args: &[&str]) -> String {
    format!("git {}", args.join(" "))
}

impl Git for ProcessGit {
    fn run(&self, repo: Option<&Path>, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut c = command(repo, args);
        c.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut child = c
            .spawn()
            .with_context(|| format!("无法启动 {}", describe(args)))?;
        if let Some(data) = stdin {
            let mut si = child.stdin.take().expect("stdin piped");
            // 单独线程写入，避免大输入时与读取 stdout 互相阻塞
            let data = data.to_vec();
            let writer = std::thread::spawn(move || si.write_all(&data));
            let out = child.wait_with_output()?;
            writer
                .join()
                .map_err(|_| anyhow!("写入 stdin 线程失败"))??;
            return check(out, args);
        }
        check(child.wait_with_output()?, args)
    }

    fn run_inherit(&self, repo: Option<&Path>, args: &[&str]) -> Result<()> {
        let st = command(repo, args)
            .status()
            .with_context(|| format!("无法启动 {}", describe(args)))?;
        if st.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "{} 退出码 {}",
                describe(args),
                st.code().unwrap_or(-1)
            ))
        }
    }
}

fn check(out: Output, args: &[&str]) -> Result<Vec<u8>> {
    if out.status.success() {
        return Ok(out.stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let mut tail: Vec<&str> = err.lines().rev().take(5).collect();
    tail.reverse();
    Err(anyhow!(
        "{} 失败（退出码 {}）\n{}",
        describe(args),
        out.status.code().unwrap_or(-1),
        tail.join("\n").trim()
    ))
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `for-each-ref --format=%(refname) <patterns>`，剔除 `*/HEAD`
pub fn for_each_ref(git: &dyn Git, repo: &Path, patterns: &[&str]) -> Result<Vec<String>> {
    let mut args = vec!["for-each-ref", "--format=%(refname)"];
    args.extend_from_slice(patterns);
    Ok(text(git.run(Some(repo), &args, None)?)
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with("/HEAD"))
        .map(str::to_string)
        .collect())
}

/// 解析 `ls-tree -r -z` 输出，只保留 blob
pub fn parse_ls_tree_z(bytes: &[u8]) -> Vec<(Oid, String)> {
    bytes
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let tab = rec.iter().position(|b| *b == b'\t')?;
            let meta = std::str::from_utf8(&rec[..tab]).ok()?;
            let mut it = meta.split(' ');
            let (_mode, ty, oid) = (it.next()?, it.next()?, it.next()?);
            if ty != "blob" {
                return None;
            }
            Some((
                Oid::new(oid),
                String::from_utf8_lossy(&rec[tab + 1..]).into_owned(),
            ))
        })
        .collect()
}

pub fn ls_tree_blobs(git: &dyn Git, repo: &Path, rev: &str) -> Result<Vec<(Oid, String)>> {
    Ok(parse_ls_tree_z(&git.run(
        Some(repo),
        &["ls-tree", "-r", "-z", rev],
        None,
    )?))
}

/// 解析 `cat-file --batch-check='%(objecttype) %(objectname) %(objectsize) %(rest)'`，只保留带路径的 blob
pub fn parse_batch_check(bytes: &[u8]) -> Vec<ObjectRec> {
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| {
            let s = String::from_utf8_lossy(line);
            let mut it = s.splitn(4, ' ');
            let (ty, oid, size, path) =
                (it.next()?, it.next()?, it.next()?, it.next().unwrap_or(""));
            if ty != "blob" || path.is_empty() {
                return None;
            }
            Some(ObjectRec {
                oid: Oid::new(oid),
                size: size.parse().ok()?,
                path: path.to_string(),
            })
        })
        .collect()
}

/// 全历史 blob 清单：`rev-list --objects --all | cat-file --batch-check`
pub fn history_objects(git: &dyn Git, repo: &Path) -> Result<Vec<ObjectRec>> {
    let listing = git.run(Some(repo), &["rev-list", "--objects", "--all"], None)?;
    let checked = git.run(
        Some(repo),
        &[
            "cat-file",
            "--batch-check=%(objecttype) %(objectname) %(objectsize) %(rest)",
        ],
        Some(&listing),
    )?;
    Ok(parse_batch_check(&checked))
}

pub fn rev_parse(git: &dyn Git, repo: &Path, rev: &str) -> Result<String> {
    Ok(
        text(git.run(Some(repo), &["rev-parse", "--verify", "-q", rev], None)?)
            .trim()
            .to_string(),
    )
}

pub fn tree_of(git: &dyn Git, repo: &Path, rev: &str) -> Result<String> {
    rev_parse(git, repo, &format!("{rev}^{{tree}}"))
}

pub fn object_exists(git: &dyn Git, repo: &Path, oid: &Oid) -> bool {
    git.run(Some(repo), &["cat-file", "-e", oid.as_str()], None)
        .is_ok()
}

/// 未设置时返回 None（git 退出码 1）
pub fn config_get(git: &dyn Git, repo: &Path, key: &str) -> Result<Option<String>> {
    match git.run(Some(repo), &["config", "--get", key], None) {
        Ok(b) => Ok(Some(text(b).trim().to_string()).filter(|s| !s.is_empty())),
        Err(_) => Ok(None),
    }
}

pub fn is_repo(git: &dyn Git, path: &Path) -> bool {
    path.is_dir()
        && git
            .run(Some(path), &["rev-parse", "--git-dir"], None)
            .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ls_tree_z() {
        let raw = b"100644 blob aaaa\tsrc.go\x00160000 commit bbbb\tsub\x00100644 blob cccc\tpkg dir/app.tar.gz\x00";
        let v = parse_ls_tree_z(raw);
        assert_eq!(
            v,
            vec![
                (Oid::new("aaaa"), "src.go".to_string()),
                (Oid::new("cccc"), "pkg dir/app.tar.gz".to_string())
            ]
        );
    }

    #[test]
    fn parses_batch_check() {
        let raw = b"commit 1111 200 \ntree 2222 30 \nblob 3333 1024 app.tar.gz\nblob 4444 5 a b.txt\nblob 5555 9 \n";
        let v = parse_batch_check(raw);
        assert_eq!(
            v,
            vec![
                ObjectRec {
                    oid: Oid::new("3333"),
                    size: 1024,
                    path: "app.tar.gz".into()
                },
                ObjectRec {
                    oid: Oid::new("4444"),
                    size: 5,
                    path: "a b.txt".into()
                },
            ]
        );
    }

    #[test]
    fn real_repo_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let g = ProcessGit;
        g.run(Some(repo), &["init", "-q", "-b", "main"], None)
            .unwrap();
        g.run(Some(repo), &["config", "user.email", "t@t.io"], None)
            .unwrap();
        g.run(Some(repo), &["config", "user.name", "t"], None)
            .unwrap();
        std::fs::write(repo.join("a.go"), "package a\n").unwrap();
        g.run(Some(repo), &["add", "-A"], None).unwrap();
        g.run(Some(repo), &["commit", "-qm", "one"], None).unwrap();
        assert!(is_repo(&g, repo));
        let refs = for_each_ref(&g, repo, &["refs/heads/"]).unwrap();
        assert_eq!(refs, vec!["refs/heads/main".to_string()]);
        let blobs = ls_tree_blobs(&g, repo, "refs/heads/main").unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].1, "a.go");
        let objs = history_objects(&g, repo).unwrap();
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].size, 10);
        assert!(object_exists(&g, repo, &blobs[0].0));
        assert!(!object_exists(
            &g,
            repo,
            &Oid::new("0000000000000000000000000000000000000000")
        ));
        assert_eq!(config_get(&g, repo, "remote.origin.url").unwrap(), None);
        let tree = tree_of(&g, repo, "main").unwrap();
        assert_eq!(tree.len(), 40);
        assert!(g.run(Some(repo), &["rev-parse", "nope"], None).is_err());
    }
}
