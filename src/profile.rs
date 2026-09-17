use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::size::parse_size;

/// 内置默认只含归档 / 包 / 磁盘镜像格式，不含图片、PDF、数据库、共享库
pub const DEFAULT_ARCHIVE_SUFFIXES: &[&str] = &[
    ".tar",
    ".tar.gz",
    ".tgz",
    ".tar.bz2",
    ".tbz2",
    ".tar.xz",
    ".txz",
    ".tar.zst",
    ".tzst",
    ".tar.lz4",
    ".zip",
    ".jar",
    ".war",
    ".ear",
    ".7z",
    ".rar",
    ".gz",
    ".bz2",
    ".xz",
    ".zst",
    ".lz4",
    ".iso",
    ".img",
    ".qcow2",
    ".ova",
    ".vmdk",
    ".squashfs",
    ".rpm",
    ".deb",
    ".whl",
    ".apk",
    ".msi",
    ".pkg",
    ".dmg",
    ".nupkg",
    ".gem",
];

pub const DEFAULT_SOURCE_SUFFIXES: &[&str] = &[
    ".go",
    ".mod",
    ".sum",
    ".proto",
    ".py",
    ".pyx",
    ".rs",
    ".java",
    ".sql",
    ".yaml",
    ".yml",
    ".json",
    ".toml",
    ".md",
    ".txt",
    ".sh",
    ".cfg",
    ".ini",
    ".tmpl",
    ".tpl",
    "Makefile",
    "Dockerfile",
];

/// 识别规则：阈值、包扩展名表、源码扩展名表。公司特有的规律放配置文件，不进代码。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Profile {
    pub min_size: String,
    pub archive_suffixes: Vec<String>,
    pub source_suffixes: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Overlay {
    min_size: Option<String>,
    archive_suffixes: Option<Vec<String>>,
    source_suffixes: Option<Vec<String>>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            min_size: "50M".into(),
            archive_suffixes: DEFAULT_ARCHIVE_SUFFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            source_suffixes: DEFAULT_SOURCE_SUFFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl Profile {
    /// 内置默认之上叠加一份 TOML（缺省字段保留默认）
    pub fn parse(text: &str) -> Result<Profile> {
        Profile::default().apply(text)
    }

    /// 只覆盖 overlay 里出现的字段
    pub fn apply(mut self, text: &str) -> Result<Profile> {
        let o: Overlay = toml::from_str(text).context("profile 解析失败")?;
        if let Some(v) = o.min_size {
            parse_size(&v)?;
            self.min_size = v;
        }
        if let Some(v) = o.archive_suffixes {
            self.archive_suffixes = v;
        }
        if let Some(v) = o.source_suffixes {
            self.source_suffixes = v;
        }
        Ok(self)
    }

    /// 内置默认 → $XDG_CONFIG_HOME/pomona/profile.toml（存在则叠加）→ cli 指定文件（必须存在）
    pub fn load(cli: Option<&Path>) -> Result<Profile> {
        let mut p = Profile::default();
        let xdg = config_dir().join("profile.toml");
        if xdg.is_file() {
            p = p
                .apply(
                    &std::fs::read_to_string(&xdg)
                        .with_context(|| format!("读取 {}", xdg.display()))?,
                )
                .with_context(|| format!("profile {}", xdg.display()))?;
        }
        if let Some(f) = cli {
            p = p
                .apply(
                    &std::fs::read_to_string(f)
                        .with_context(|| format!("读取 profile {}", f.display()))?,
                )
                .with_context(|| format!("profile {}", f.display()))?;
        }
        Ok(p)
    }

    pub fn min_size_bytes(&self) -> Result<u64> {
        parse_size(&self.min_size)
    }

    pub fn is_archive(&self, path: &str) -> bool {
        suffix_hit(&self.archive_suffixes, path)
    }

    pub fn is_source(&self, path: &str) -> bool {
        suffix_hit(&self.source_suffixes, path)
    }
}

/// 以 `.` 开头的按后缀匹配（忽略大小写）；否则按文件名整体匹配（Makefile）
fn suffix_hit(list: &[String], path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    list.iter().any(|s| {
        let s = s.to_ascii_lowercase();
        if s.starts_with('.') {
            lower.ends_with(&s)
        } else {
            base == s
        }
    })
}

pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("pomona");
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".config")
        .join("pomona")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_archives_not_images() {
        let p = Profile::default();
        assert!(p.is_archive("dist/app.tar.gz"));
        assert!(p.is_archive("X.ZIP"));
        assert!(p.is_archive("pkg.rpm"));
        assert!(!p.is_archive("logo.png"));
        assert!(!p.is_archive("doc.pdf"));
        assert!(!p.is_archive("data.db"));
        assert!(p.is_source("main.go"));
        assert!(p.is_source("build/Makefile"));
        assert!(!p.is_source("app.tar.gz"));
        assert_eq!(p.min_size_bytes().unwrap(), 50 * 1024 * 1024);
    }

    #[test]
    fn overlay_only_overrides_present_fields() {
        let p = Profile::default()
            .apply("min_size = \"10M\"\narchive_suffixes = [\".bundle\"]\n")
            .unwrap();
        assert_eq!(p.min_size_bytes().unwrap(), 10 * 1024 * 1024);
        assert!(p.is_archive("x.bundle"));
        assert!(!p.is_archive("x.tar.gz"));
        assert!(p.is_source("main.go"));
        assert!(Profile::default().apply("min_size = 5").is_err());
        assert!(Profile::default().apply("min_size = \"5X\"").is_err());
        assert!(Profile::default().apply("unknown = 1").is_err());
    }

    #[test]
    fn load_layers_from_files() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("p.toml");
        std::fs::write(&f, "source_suffixes = [\".rs\"]\n").unwrap();
        let p = Profile::load(Some(&f)).unwrap();
        assert!(p.is_source("a.rs"));
        assert!(!p.is_source("a.go"));
        assert!(Profile::load(Some(&tmp.path().join("missing.toml"))).is_err());
    }
}
