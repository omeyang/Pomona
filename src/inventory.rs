use std::path::Path;

use anyhow::{Result, anyhow};

use crate::git::{self, Git};
use crate::model::{Inventory, SortBy};

/// 保留基准：branch_refs 提供 tip 索引与 KEEP；extra_refs（如 refs/tags/*）只贡献 KEEP
pub struct KeepRefs {
    pub branch_refs: Vec<String>,
    /// 从 ref 名剥掉的前缀，剩下的作为分支显示名（refs/remotes/origin/ 或 refs/heads/）
    pub branch_prefix: String,
    pub extra_refs: Vec<String>,
}

pub fn expand_globs(g: &dyn Git, repo: &Path, globs: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for glob in globs {
        out.extend(git::for_each_ref(g, repo, &[glob.as_str()])?);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub fn build(g: &dyn Git, repo: &Path, keep: &KeepRefs, sort: SortBy) -> Result<Inventory> {
    if keep.branch_refs.is_empty() {
        return Err(anyhow!("没有任何分支 ref 可作为保留基准"));
    }
    let objects = git::history_objects(g, repo)?;
    let mut inv = Inventory::from_objects(objects, sort);
    for r in &keep.branch_refs {
        let label = r.strip_prefix(&keep.branch_prefix).unwrap_or(r).to_string();
        for (oid, path) in git::ls_tree_blobs(g, repo, r)? {
            inv.tips
                .entry(path.clone())
                .or_default()
                .insert(label.clone());
            inv.keep_blobs.insert(oid);
            inv.keep_paths.insert(path);
        }
    }
    for r in &keep.extra_refs {
        for (oid, path) in git::ls_tree_blobs(g, repo, r)? {
            inv.keep_blobs.insert(oid);
            inv.keep_paths.insert(path);
        }
    }
    Ok(inv)
}
