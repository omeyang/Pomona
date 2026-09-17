use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::git::{self, Git};
use crate::model::Oid;

pub const PUSH_INFO_VERSION: u32 = 1;

/// clean 交给 push 的信息：远程 URL、重写前各 keep ref 的树 SHA、待删 blob 列表
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushInfo {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub branches: Vec<String>,
    #[serde(default)]
    pub tips: BTreeMap<String, String>,
    #[serde(default)]
    pub removed: Vec<String>,
    #[serde(default)]
    pub reclaim_bytes: u64,
}

impl PushInfo {
    /// `<镜像>.pomona-push.toml`，与镜像目录并排
    pub fn path_for(mirror: &Path) -> PathBuf {
        let mut name = mirror
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".pomona-push.toml");
        mirror.with_file_name(name)
    }

    pub fn load(path: &Path) -> Result<PushInfo> {
        let info: PushInfo = toml::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("读取 {}", path.display()))?,
        )
        .context("push 信息解析失败")?;
        if info.version != PUSH_INFO_VERSION {
            return Err(anyhow!("push 信息 version {} 不受支持", info.version));
        }
        Ok(info)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, toml::to_string_pretty(self).expect("可序列化"))
            .with_context(|| format!("写 {}", path.display()))
    }
}

/// ref -> 树对象 SHA
pub fn snapshot_tips(
    g: &dyn Git,
    repo: &Path,
    refs: &[String],
) -> Result<BTreeMap<String, String>> {
    refs.iter()
        .map(|r| Ok((r.clone(), git::tree_of(g, repo, r)?)))
        .collect()
}

/// 安全不变量：每个 keep ref 的树未变；待删 blob 均不可达
pub fn verify(g: &dyn Git, mirror: &Path, info: &PushInfo) -> Result<()> {
    for (r, tree) in &info.tips {
        let now = git::tree_of(g, mirror, r).with_context(|| format!("校验 {r}"))?;
        if &now != tree {
            return Err(anyhow!(
                "校验失败: {r} 的树对象已改变（期望 {tree}，实际 {now}）"
            ));
        }
    }
    for o in &info.removed {
        if git::object_exists(g, mirror, &Oid::new(o.as_str())) {
            return Err(anyhow!("校验失败: 待删 blob {o} 仍然存在"));
        }
    }
    Ok(())
}

/// 某 ref 树里最大的 n 个文件（人工核对包仍在）
pub fn largest_files(g: &dyn Git, repo: &Path, rev: &str, n: usize) -> Result<Vec<(u64, String)>> {
    let raw = g.run(Some(repo), &["ls-tree", "-r", "-l", "-z", rev], None)?;
    let mut v: Vec<(u64, String)> = raw
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let tab = rec.iter().position(|b| *b == b'\t')?;
            let meta = String::from_utf8_lossy(&rec[..tab]);
            let mut it = meta.split_whitespace();
            let (_mode, ty, _oid, size) = (it.next()?, it.next()?, it.next()?, it.next()?);
            if ty != "blob" {
                return None;
            }
            Some((
                size.parse().ok()?,
                String::from_utf8_lossy(&rec[tab + 1..]).into_owned(),
            ))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    v.truncate(n);
    Ok(v)
}

pub fn push_args(url: &str, mirror_mode: bool) -> Vec<String> {
    if mirror_mode {
        vec![
            "push".into(),
            "--mirror".into(),
            "--force".into(),
            url.into(),
        ]
    } else {
        vec![
            "push".into(),
            "--force".into(),
            url.into(),
            "refs/heads/*:refs/heads/*".into(),
            "refs/tags/*:refs/tags/*".into(),
        ]
    }
}
