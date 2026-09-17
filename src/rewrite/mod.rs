pub mod filter_repo;
pub mod native;
pub mod stream;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::git::Git;
use crate::model::Oid;

#[derive(Debug, Default)]
pub struct RewriteReport {
    /// 被改写的文件变更条目数（filter-repo 引擎无法统计，返回待删 blob 数）
    pub rewritten_entries: usize,
    /// 失败时保留的过滤后流文件
    pub stream_log: Option<PathBuf>,
}

/// 历史重写引擎：删除指定 blob，保持提交拓扑
pub trait Rewriter {
    fn name(&self) -> &'static str;
    fn strip_blobs(&self, repo: &Path, remove: &BTreeSet<Oid>) -> Result<RewriteReport>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Engine {
    /// Rust 原生：fast-export 流过滤 → fast-import（默认，无需 Python）
    Native,
    /// 调用 git-filter-repo（需 pip 安装）
    FilterRepo,
}

pub fn rewriter_for(engine: Engine) -> Box<dyn Rewriter> {
    match engine {
        Engine::Native => Box::new(native::NativeRewriter),
        Engine::FilterRepo => Box::new(filter_repo::FilterRepoRewriter),
    }
}

/// 让不可达对象真正被回收
pub fn expire_and_gc(git: &dyn Git, repo: &Path) -> Result<()> {
    git.run(
        Some(repo),
        &["reflog", "expire", "--expire=now", "--all"],
        None,
    )?;
    git.run(Some(repo), &["gc", "--prune=now", "--quiet"], None)?;
    Ok(())
}
