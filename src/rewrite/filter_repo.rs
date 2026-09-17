use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};

use super::{RewriteReport, Rewriter, expire_and_gc};
use crate::git::ProcessGit;
use crate::model::Oid;

/// 回退引擎：`git filter-repo --strip-blobs-with-ids`
pub struct FilterRepoRewriter;

pub fn available() -> bool {
    Command::new("git")
        .args(["filter-repo", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl Rewriter for FilterRepoRewriter {
    fn name(&self) -> &'static str {
        "filter-repo"
    }

    fn strip_blobs(&self, repo: &Path, remove: &BTreeSet<Oid>) -> Result<RewriteReport> {
        if !available() {
            return Err(anyhow!(
                "git-filter-repo 未安装（pip3.12 install git-filter-repo），或改用 --engine native"
            ));
        }
        let mut ids = tempfile::NamedTempFile::new().context("创建临时文件")?;
        for o in remove {
            writeln!(ids, "{o}")?;
        }
        ids.flush()?;
        let st = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["filter-repo", "--strip-blobs-with-ids"])
            .arg(ids.path())
            .args([
                "--prune-empty",
                "never",
                "--prune-degenerate",
                "never",
                "--force",
            ])
            .status()
            .context("启动 git filter-repo")?;
        if !st.success() {
            return Err(anyhow!(
                "git filter-repo 失败（退出码 {}）",
                st.code().unwrap_or(-1)
            ));
        }
        expire_and_gc(&ProcessGit, repo)?;
        Ok(RewriteReport {
            rewritten_entries: remove.len(),
            stream_log: None,
        })
    }
}
