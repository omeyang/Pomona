use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::git::{self, Git};

/// 镜像来源：在线（复用本地对象）、仅 URL、纯离线
pub enum Source {
    Online { url: String, local: PathBuf },
    UrlOnly { url: String },
    Offline { local: PathBuf },
}

impl Source {
    pub fn resolve(url: Option<&str>, local: Option<&Path>) -> Result<Source> {
        let url = url
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(str::to_string);
        let local = local.filter(|p| p.is_dir()).map(Path::to_path_buf);
        match (url, local) {
            (Some(url), Some(local)) => Ok(Source::Online { url, local }),
            (Some(url), None) => Ok(Source::UrlOnly { url }),
            (None, Some(local)) => Ok(Source::Offline { local }),
            (None, None) => Err(anyhow!(
                "plan 里既无 source.url，source.local 也不是可用目录"
            )),
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Source::Online { url, .. } | Source::UrlOnly { url } => Some(url),
            Source::Offline { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Source::Online { url, local } => {
                format!(
                    "在线：从 {url} 取全部分支，复用本地对象 {}",
                    local.display()
                )
            }
            Source::UrlOnly { url } => format!("在线：从 {url} 完整镜像克隆"),
            Source::Offline { local } => {
                format!(
                    "离线：镜像本地克隆 {}，用 origin/* 重建分支",
                    local.display()
                )
            }
        }
    }
}

/// 建镜像，返回 refs/heads 下的分支名列表；workdir 已存在则报错
pub fn build(g: &dyn Git, src: &Source, workdir: &Path) -> Result<Vec<String>> {
    if workdir.exists() {
        return Err(anyhow!("workdir 已存在: {}", workdir.display()));
    }
    let w = workdir
        .to_str()
        .ok_or_else(|| anyhow!("workdir 路径不是 UTF-8"))?;
    match src {
        Source::Online { url, local } => {
            let l = local
                .to_str()
                .ok_or_else(|| anyhow!("本地路径不是 UTF-8"))?;
            g.run_inherit(
                None,
                &[
                    "clone",
                    "--mirror",
                    "--reference",
                    l,
                    "--dissociate",
                    url,
                    w,
                ],
            )?;
        }
        Source::UrlOnly { url } => g.run_inherit(None, &["clone", "--mirror", url, w])?,
        Source::Offline { local } => {
            let l = local
                .to_str()
                .ok_or_else(|| anyhow!("本地路径不是 UTF-8"))?;
            g.run_inherit(None, &["clone", "--mirror", l, w])?;
            remap_remotes_to_heads(g, workdir)?;
        }
    }
    let heads = git::for_each_ref(g, workdir, &["refs/heads/"])?;
    if heads.is_empty() {
        return Err(anyhow!("镜像里没有任何分支"));
    }
    Ok(heads
        .into_iter()
        .map(|r| r.trim_start_matches("refs/heads/").to_string())
        .collect())
}

/// 离线模式：refs/remotes/<remote>/<branch> → refs/heads/<branch>，删掉原来的 heads 与 remotes
fn remap_remotes_to_heads(g: &dyn Git, w: &Path) -> Result<()> {
    for r in git::for_each_ref(g, w, &["refs/heads/"])? {
        g.run(Some(w), &["update-ref", "-d", &r], None)?;
    }
    // 先删 refs/remotes/*/HEAD 这类符号引用；必须 --no-deref，否则会删掉它指向的分支
    let raw = g.run(
        Some(w),
        &["for-each-ref", "--format=%(refname)", "refs/remotes/"],
        None,
    )?;
    for r in String::from_utf8_lossy(&raw)
        .lines()
        .filter(|l| l.ends_with("/HEAD"))
    {
        g.run(Some(w), &["update-ref", "--no-deref", "-d", r], None)?;
    }
    let remotes = git::for_each_ref(g, w, &["refs/remotes/"])?;
    for r in &remotes {
        let rest = r.trim_start_matches("refs/remotes/");
        let Some((_, branch)) = rest.split_once('/') else {
            continue;
        };
        g.run(
            Some(w),
            &["update-ref", &format!("refs/heads/{branch}"), r],
            None,
        )?;
    }
    for r in &remotes {
        g.run(Some(w), &["update-ref", "-d", r], None)?;
    }
    Ok(())
}
