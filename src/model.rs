use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// git 对象名（十六进制，40 或 64 位）
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Oid(String);

impl Oid {
    pub fn new(s: impl Into<String>) -> Self {
        Oid(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 「前 N 大」的排序口径
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    /// 单版本最大
    Max,
    /// 所有版本累计
    Cum,
}

/// 某路径在全历史里的体积统计
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathStats {
    pub path: String,
    pub max: u64,
    pub cum: u64,
    pub versions: u32,
}

/// 全历史里的一个 blob（首次出现时的路径）
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRec {
    pub oid: Oid,
    pub size: u64,
    pub path: String,
}

/// 一次分析的全部素材：路径统计、tip 索引、KEEP 集合与全历史对象清单
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub stats: Vec<PathStats>,
    /// 路径 -> 哪些分支的 tip 仍引用它
    pub tips: BTreeMap<String, BTreeSet<String>>,
    /// 所有 keep ref 树里的 blob，绝不删除
    pub keep_blobs: BTreeSet<Oid>,
    /// 所有 keep ref 树里的路径
    pub keep_paths: BTreeSet<String>,
    pub objects: Vec<ObjectRec>,
}

impl Inventory {
    pub fn from_objects(objects: Vec<ObjectRec>, sort: SortBy) -> Self {
        let mut agg: BTreeMap<&str, PathStats> = BTreeMap::new();
        for o in &objects {
            let e = agg.entry(o.path.as_str()).or_insert_with(|| PathStats {
                path: o.path.clone(),
                max: 0,
                cum: 0,
                versions: 0,
            });
            e.cum += o.size;
            e.versions += 1;
            e.max = e.max.max(o.size);
        }
        let stats = agg.into_values().collect();
        let mut inv = Inventory {
            stats,
            objects,
            ..Default::default()
        };
        inv.sort(sort);
        inv
    }

    pub fn sort(&mut self, by: SortBy) {
        self.stats.sort_by(|a, b| {
            let primary = match by {
                SortBy::Max => b.max.cmp(&a.max).then_with(|| b.cum.cmp(&a.cum)),
                SortBy::Cum => b.cum.cmp(&a.cum).then_with(|| b.max.cmp(&a.max)),
            };
            primary.then_with(|| a.path.cmp(&b.path))
        });
    }

    pub fn tip_branches(&self, path: &str) -> Vec<String> {
        self.tips
            .get(path)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 历史里出现过，但不在任何 keep ref 树里
    pub fn is_orphan(&self, path: &str) -> bool {
        !self.keep_paths.contains(path)
    }

    pub fn orphans(&self) -> Vec<String> {
        self.stats
            .iter()
            .filter(|s| self.is_orphan(&s.path))
            .map(|s| s.path.clone())
            .collect()
    }
}

/// 某路径被选中的原因
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Orphan,
    Size,
    Suffix,
    Regex,
    Manual,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Orphan => "orphan",
            Reason::Size => "size",
            Reason::Suffix => "suffix",
            Reason::Regex => "regex",
            Reason::Manual => "manual",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Reason::Orphan => "孤儿路径",
            Reason::Size => "超阈值",
            Reason::Suffix => "包扩展名",
            Reason::Regex => "正则命中",
            Reason::Manual => "人工选中",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Reason {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "orphan" => Reason::Orphan,
            "size" => Reason::Size,
            "suffix" => Reason::Suffix,
            "regex" => Reason::Regex,
            "manual" => Reason::Manual,
            _ => return Err(format!("未知 reason: {s}")),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub path: String,
    pub reason: Reason,
}

/// 待删 blob 集合与预估
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removal {
    pub remove: BTreeSet<Oid>,
    /// 选中路径里因在 tip 而保留的 blob 数
    pub kept: usize,
    pub reclaim_bytes: u64,
}
