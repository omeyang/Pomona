use std::collections::BTreeSet;
use std::io::Write;

use anyhow::Result;
use regex::Regex;

use crate::model::{Inventory, PathStats, Pick, Reason, SortBy};
use crate::profile::Profile;
use crate::prompt::{Answer, Prompt};
use crate::size::human;
use crate::term::{Color, paint};

/// 非交互选路规则
pub struct RuleSet<'a> {
    pub orphans: bool,
    pub auto_detect: bool,
    pub regexes: Vec<Regex>,
    pub min_size: u64,
    pub profile: &'a Profile,
}

/// 是否算「包 / 大文件」：单版本超阈值 或 命中包扩展名
pub fn qualifies(max: u64, path: &str, min_size: u64, profile: &Profile) -> bool {
    max >= min_size || profile.is_archive(path)
}

/// 顺序：孤儿 → 自动检测 / 正则；按路径去重
pub fn apply_rules(inv: &Inventory, rules: &RuleSet) -> Vec<Pick> {
    let mut seen = BTreeSet::new();
    let mut picks = Vec::new();
    let mut push = |path: &str, reason: Reason, picks: &mut Vec<Pick>| {
        if seen.insert(path.to_string()) {
            picks.push(Pick {
                path: path.to_string(),
                reason,
            });
        }
    };
    if rules.orphans {
        for p in inv.orphans() {
            push(&p, Reason::Orphan, &mut picks);
        }
    }
    for s in &inv.stats {
        if rules.auto_detect {
            if s.max >= rules.min_size {
                push(&s.path, Reason::Size, &mut picks);
                continue;
            }
            if rules.profile.is_archive(&s.path) {
                push(&s.path, Reason::Suffix, &mut picks);
                continue;
            }
        }
        if rules.regexes.iter().any(|re| re.is_match(&s.path)) {
            push(&s.path, Reason::Regex, &mut picks);
        }
    }
    picks
}

pub struct ReviewOpts {
    /// None = 审阅全部
    pub top: Option<usize>,
    pub sort: SortBy,
}

pub fn reasons(inv: &Inventory, s: &PathStats, min_size: u64, profile: &Profile) -> Vec<String> {
    let mut r = Vec::new();
    if s.max >= min_size {
        r.push(format!("超阈值({})", human(s.max)));
    }
    if profile.is_archive(&s.path) {
        r.push("包扩展名".into());
    }
    if profile.is_source(&s.path) {
        r.push("源码".into());
    }
    let n = inv.tip_branches(&s.path).len();
    if n > 0 {
        r.push(format!("{n} 个分支的最新版仍在用"));
    } else {
        r.push("无分支引用(纯历史遗留)".into());
    }
    r
}

/// 逐个审阅前 N 大；已被规则选中或孤儿的只展示不询问
pub fn review(
    inv: &Inventory,
    already: &[Pick],
    rules: &RuleSet,
    opts: &ReviewOpts,
    prompt: &mut dyn Prompt,
    out: &mut dyn Write,
) -> Result<Vec<Pick>> {
    let picked: BTreeSet<&str> = already.iter().map(|p| p.path.as_str()).collect();
    let mut sorted = inv.clone();
    sorted.sort(opts.sort);
    let total = opts
        .top
        .map_or(sorted.stats.len(), |n| n.min(sorted.stats.len()));
    let ord = match opts.sort {
        SortBy::Max => "按单版本最大",
        SortBy::Cum => "按累计体积",
    };
    writeln!(
        out,
        "{}",
        paint(
            Color::Yellow,
            &format!(
                "[选路] 逐个审阅前 {total} 项({ord})  Y=保留各分支最新版/删历史旧版  n=不动  q=结束"
            )
        )
    )?;
    writeln!(out)?;
    let mut picks = Vec::new();
    for (i, s) in sorted.stats.iter().take(total).enumerate() {
        let idx = i + 1;
        writeln!(
            out,
            "{} {}",
            paint(Color::Cyan, &format!("[{idx}/{total}]")),
            s.path
        )?;
        if picked.contains(s.path.as_str()) {
            writeln!(out, "  -> {}\n", paint(Color::Red, "已选中(规则命中)"))?;
            continue;
        }
        if rules.orphans && inv.is_orphan(&s.path) {
            writeln!(
                out,
                "  -> {}\n",
                paint(Color::Red, "孤儿(无分支引用), 已自动标记清理")
            )?;
            continue;
        }
        let branches = inv.tip_branches(&s.path);
        let dir = s.path.rsplit_once('/').map_or("(仓库根目录)", |(d, _)| d);
        writeln!(out, "  目录: {dir}")?;
        writeln!(
            out,
            "  单版本最大: {}  累计 {} ({} 个历史版本)",
            paint(Color::Yellow, &human(s.max)),
            human(s.cum),
            s.versions
        )?;
        if branches.is_empty() {
            writeln!(
                out,
                "  当前还在用: {}  (没有任何分支引用, 全是历史旧版)",
                paint(Color::Red, "无")
            )?;
        } else {
            writeln!(
                out,
                "  当前还在用: {}  (这些分支的最新版会原样保留)",
                paint(Color::Green, &branches.join(" "))
            )?;
        }
        writeln!(
            out,
            "  理由: {}",
            reasons(inv, s, rules.min_size, rules.profile).join(", ")
        )?;
        out.flush()?;
        let rec = qualifies(s.max, &s.path, rules.min_size, rules.profile);
        let q = if rec {
            format!("  建议 {}", paint(Color::Red, "清理"))
        } else {
            format!("  建议 {}", paint(Color::Green, "保留"))
        };
        match prompt.confirm(&q, rec)? {
            Answer::Yes => {
                writeln!(out, "  -> {}\n", paint(Color::Red, "标记清理"))?;
                picks.push(Pick {
                    path: s.path.clone(),
                    reason: Reason::Manual,
                });
            }
            Answer::No => writeln!(out, "  -> {}\n", paint(Color::Green, "保留"))?,
            Answer::Quit => {
                writeln!(out, "  -> 结束")?;
                break;
            }
        }
    }
    Ok(picks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::prompt::{Answer, ScriptedPrompt};

    fn inv() -> Inventory {
        let objects = vec![
            ObjectRec {
                oid: Oid::new("a1"),
                size: 100,
                path: "app.tar.gz".into(),
            },
            ObjectRec {
                oid: Oid::new("a2"),
                size: 900,
                path: "app.tar.gz".into(),
            },
            ObjectRec {
                oid: Oid::new("b1"),
                size: 5000,
                path: "bundle".into(),
            },
            ObjectRec {
                oid: Oid::new("o1"),
                size: 7,
                path: "old/gone.bin".into(),
            },
            ObjectRec {
                oid: Oid::new("s1"),
                size: 10,
                path: "src.go".into(),
            },
        ];
        let mut inv = Inventory::from_objects(objects, SortBy::Max);
        for p in ["app.tar.gz", "bundle", "src.go"] {
            inv.keep_paths.insert(p.into());
            inv.tips.entry(p.into()).or_default().insert("main".into());
        }
        inv
    }

    #[test]
    fn auto_detect_by_size_or_suffix() {
        let p = Profile::default();
        let rules = RuleSet {
            orphans: false,
            auto_detect: true,
            regexes: vec![],
            min_size: 1000,
            profile: &p,
        };
        let picks = apply_rules(&inv(), &rules);
        let paths: Vec<_> = picks.iter().map(|p| (p.path.as_str(), p.reason)).collect();
        assert_eq!(
            paths,
            vec![("bundle", Reason::Size), ("app.tar.gz", Reason::Suffix)]
        );
    }

    #[test]
    fn orphans_first_then_regex() {
        let p = Profile::default();
        let rules = RuleSet {
            orphans: true,
            auto_detect: false,
            regexes: vec![Regex::new("^src").unwrap()],
            min_size: 1000,
            profile: &p,
        };
        let picks = apply_rules(&inv(), &rules);
        let paths: Vec<_> = picks.iter().map(|p| (p.path.as_str(), p.reason)).collect();
        assert_eq!(
            paths,
            vec![("old/gone.bin", Reason::Orphan), ("src.go", Reason::Regex)]
        );
    }

    #[test]
    fn review_walks_top_n_and_honours_answers() {
        let p = Profile::default();
        let rules = RuleSet {
            orphans: false,
            auto_detect: false,
            regexes: vec![],
            min_size: 1000,
            profile: &p,
        };
        // 排序(Max): bundle(5000) app(900) src(10) gone(7)；top 3；回答: Y, n, q
        let mut prompt = ScriptedPrompt::new(vec![Answer::Yes, Answer::No, Answer::Quit], vec![]);
        let mut out = Vec::new();
        let picks = review(
            &inv(),
            &[],
            &rules,
            &ReviewOpts {
                top: Some(3),
                sort: SortBy::Max,
            },
            &mut prompt,
            &mut out,
        )
        .unwrap();
        assert_eq!(
            picks,
            vec![Pick {
                path: "bundle".into(),
                reason: Reason::Manual
            }]
        );
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("[1/3]") && text.contains("bundle"));
        assert!(text.contains("[3/3]"), "第三项会展示后再被 q 结束");
        assert!(!text.contains("[4/"));
    }

    #[test]
    fn review_skips_already_picked_and_orphans() {
        let p = Profile::default();
        let rules = RuleSet {
            orphans: true,
            auto_detect: false,
            regexes: vec![],
            min_size: 1000,
            profile: &p,
        };
        let already = vec![Pick {
            path: "bundle".into(),
            reason: Reason::Size,
        }];
        let mut prompt = ScriptedPrompt::new(vec![Answer::No, Answer::No], vec![]);
        let mut out = Vec::new();
        let picks = review(
            &inv(),
            &already,
            &rules,
            &ReviewOpts {
                top: None,
                sort: SortBy::Max,
            },
            &mut prompt,
            &mut out,
        )
        .unwrap();
        assert!(picks.is_empty());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("孤儿"));
        assert!(text.contains("已选中"));
    }

    #[test]
    fn reasons_describe_candidate() {
        let p = Profile::default();
        let i = inv();
        let s = i.stats.iter().find(|s| s.path == "app.tar.gz").unwrap();
        let r = reasons(&i, s, 1000, &p);
        assert!(r.iter().any(|x| x.contains("包扩展名")));
        assert!(r.iter().any(|x| x.contains("1 个分支")));
    }
}
