use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};

use super::{RewriteReport, Rewriter, expire_and_gc, stream::filter_stream};
use crate::git::ProcessGit;
use crate::model::Oid;

/// `git fast-export --no-data` → 流过滤 → `git fast-import --force`，同一仓库内原地完成。
/// 验证过的性质：tip 树不变；不涉及待删 blob 的提交 SHA 不变；合并提交与附注标签保留。
pub struct NativeRewriter;

/// 同时写两个目标：fast-import 的 stdin 与一份日志文件（失败时排查用）
struct Tee<A: Write, B: Write>(A, B);

impl<A: Write, B: Write> Write for Tee<A, B> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write_all(buf)?;
        self.1.write_all(buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()?;
        self.1.flush()
    }
}

impl Rewriter for NativeRewriter {
    fn name(&self) -> &'static str {
        "native"
    }

    fn strip_blobs(&self, repo: &Path, remove: &BTreeSet<Oid>) -> Result<RewriteReport> {
        let log_path =
            std::env::temp_dir().join(format!("pomona-stream-{}.txt", std::process::id()));
        let mut export = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "fast-export",
                "--all",
                "--no-data",
                "--reencode=no",
                "--signed-tags=strip",
                "--use-done-feature",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("启动 git fast-export")?;
        let mut import = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["fast-import", "--force", "--quiet"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("启动 git fast-import")?;
        let filtered = {
            let reader = BufReader::new(export.stdout.take().expect("stdout piped"));
            let sink = BufWriter::new(import.stdin.take().expect("stdin piped"));
            let log = BufWriter::new(
                File::create(&log_path).with_context(|| format!("创建 {}", log_path.display()))?,
            );
            filter_stream(reader, remove, Tee(sink, log))
        }; // 作用域结束：关闭 fast-import 的 stdin
        let es = export.wait()?;
        let is = import.wait()?;
        let rewritten = match filtered {
            Ok(n) => n,
            Err(e) => return Err(e.context(format!("过滤后的流保存在 {}", log_path.display()))),
        };
        if !es.success() {
            return Err(anyhow!(
                "git fast-export 失败（退出码 {}），过滤后的流保存在 {}",
                es.code().unwrap_or(-1),
                log_path.display()
            ));
        }
        if !is.success() {
            return Err(anyhow!(
                "git fast-import 失败（退出码 {}），过滤后的流保存在 {}",
                is.code().unwrap_or(-1),
                log_path.display()
            ));
        }
        expire_and_gc(&ProcessGit, repo)?;
        let _ = std::fs::remove_file(&log_path);
        Ok(RewriteReport {
            rewritten_entries: rewritten,
            stream_log: None,
        })
    }
}
