use std::io::Write;

use anyhow::{Result, anyhow};
use regex::Regex;

use crate::cli::AnalyzeArgs;
use crate::git::{self, ProcessGit};
use crate::inventory::{KeepRefs, build, expand_globs};
use crate::model::{Pick, Reason, SortBy};
use crate::plan::{PLAN_VERSION, Plan, PlanPath, PlanPolicy, PlanSource};
use crate::profile::Profile;
use crate::prompt::{Prompt, TerminalPrompt};
use crate::removal::plan_removal;
use crate::select::{ReviewOpts, RuleSet, apply_rules, review};
use crate::size::{human, parse_size};
use crate::term::{Color, banner, paint};

fn is_url(s: &str) -> bool {
    s.contains("://") || (s.contains('@') && s.contains(':'))
}

pub fn run(a: AnalyzeArgs) -> Result<()> {
    let repo_str = a.repo.to_string_lossy().to_string();
    if is_url(&repo_str) {
        return Err(anyhow!(
            "analyze 需要一个本地仓库目录（它会从中读 origin/* 全历史）。若只有 URL，请先 git clone 到本地再指向该目录。"
        ));
    }
    if !a.repo.is_dir() {
        return Err(anyhow!("目录不存在: {}", a.repo.display()));
    }
    let g = ProcessGit;
    if !git::is_repo(&g, &a.repo) {
        return Err(anyhow!("不是 git 仓库: {}", a.repo.display()));
    }
    let repo = a.repo.canonicalize()?;
    let profile = Profile::load(a.profile.as_deref())?;
    let min_size = match &a.min_size {
        Some(s) => parse_size(s)?,
        None => profile.min_size_bytes()?,
    };
    let sort = if a.sort_by == "cum" {
        SortBy::Cum
    } else {
        SortBy::Max
    };
    let regexes = a
        .auto_select
        .iter()
        .map(|r| Regex::new(r).map_err(|e| anyhow!("正则不合法 {r}: {e}")))
        .collect::<Result<Vec<_>>>()?;
    let noninteractive = a.auto_detect || !regexes.is_empty();
    let source_url = git::config_get(&g, &repo, &format!("remote.{}.url", a.remote))?;

    banner(&[
        " ① 清理前分析（只读）".into(),
        format!(" 本地仓库: {}", repo.display()),
        format!(
            " remote({}) URL: {}",
            a.remote,
            source_url.as_deref().unwrap_or("<未配置>")
        ),
    ]);
    if source_url.is_none() {
        println!(
            "{}",
            paint(
                Color::Yellow,
                &format!(
                    "提示: 未读到 remote.{}.url，plan 里 URL 将为空；clean 时按离线模式处理。",
                    a.remote
                )
            )
        );
    }
    println!(
        "{}",
        paint(
            Color::Yellow,
            "提示: 分析基于本地已 fetch 的状态。建议先执行 git fetch --all --prune。"
        )
    );
    println!();

    let prefix = format!("refs/remotes/{}/", a.remote);
    let branch_refs = expand_globs(&g, &repo, std::slice::from_ref(&prefix))?;
    if branch_refs.is_empty() {
        return Err(anyhow!("在 {prefix} 下没找到任何分支。先 git fetch？"));
    }
    let extra_refs = expand_globs(&g, &repo, &a.keep_refs)?;
    println!(
        "{}",
        paint(Color::Cyan, &format!(" 远程分支数: {}", branch_refs.len()))
    );
    println!();
    let inv = build(
        &g,
        &repo,
        &KeepRefs {
            branch_refs,
            branch_prefix: prefix,
            extra_refs,
        },
        sort,
    )?;
    if inv.stats.is_empty() {
        println!("仓库里没有文件？退出。");
        return Ok(());
    }

    let rules = RuleSet {
        orphans: a.orphans,
        auto_detect: a.auto_detect,
        regexes,
        min_size,
        profile: &profile,
    };
    let mut picks: Vec<Pick> = apply_rules(&inv, &rules);
    if a.orphans {
        let n = picks.iter().filter(|p| p.reason == Reason::Orphan).count();
        if n > 0 {
            println!(
                "{}",
                paint(
                    Color::Yellow,
                    &format!("[孤儿] 自动选中 {n} 个无任何分支引用的路径（无视大小）")
                )
            );
        } else {
            println!(
                "{}",
                paint(Color::Green, "[孤儿] 没有发现无分支引用的孤儿路径")
            );
        }
        println!();
    }
    if noninteractive {
        println!(
            "{}",
            paint(
                Color::Yellow,
                &format!(
                    "[选路] 自动识别（单版本 ≥ {} 或 命中包扩展名）",
                    human(min_size)
                )
            )
        );
        for p in picks.iter().filter(|p| p.reason != Reason::Orphan) {
            let s = inv
                .stats
                .iter()
                .find(|s| s.path == p.path)
                .expect("stats 含所有路径");
            let brs = inv.tip_branches(&p.path);
            println!(
                "  {} {}  单版本{}/累计{} ({}版本; {}; 当前在用: {})",
                paint(Color::Red, "选中"),
                p.path,
                human(s.max),
                human(s.cum),
                s.versions,
                p.reason.label(),
                if brs.is_empty() {
                    "无".to_string()
                } else {
                    brs.join(",")
                }
            );
        }
    } else {
        let mut prompt = TerminalPrompt;
        let top = if a.all {
            None
        } else {
            Some(match a.top {
                Some(n) => n,
                None => prompt.number("输入 top 数字(按大小排前 N 大)", 30)?,
            })
        };
        let mut out = std::io::stdout().lock();
        let more = review(
            &inv,
            &picks,
            &rules,
            &ReviewOpts { top, sort },
            &mut prompt,
            &mut out,
        )?;
        out.flush()?;
        picks.extend(more);
    }
    if picks.is_empty() {
        println!("没有选中任何路径，未生成 plan。");
        return Ok(());
    }

    let selected: Vec<String> = picks.iter().map(|p| p.path.clone()).collect();
    let removal = plan_removal(&inv, &selected);
    println!();
    println!(
        "{}",
        paint(Color::Cyan, "---------- 分析结果（预览） ----------")
    );
    println!("  选中路径: {} 个", picks.len());
    println!(
        "  历史里待删 blob: {}  预计可回收: {}",
        paint(Color::Red, &removal.remove.len().to_string()),
        paint(Color::Green, &human(removal.reclaim_bytes))
    );
    println!("  （各分支 tip 当前的包都会保留）");
    println!();

    let plan = Plan {
        version: PLAN_VERSION,
        generated_by: format!("pomona {}", env!("CARGO_PKG_VERSION")),
        source: PlanSource {
            url: source_url,
            local: Some(repo.to_string_lossy().to_string()),
            remote: a.remote.clone(),
        },
        policy: PlanPolicy {
            min_size,
            keep_refs: a.keep_refs.clone(),
        },
        paths: picks
            .iter()
            .map(|p| PlanPath {
                path: p.path.clone(),
                reason: p.reason.to_string(),
            })
            .collect(),
    };
    plan.save(
        &a.out,
        &[format!(
            "预计可回收: {}  待删历史 blob: {}",
            human(removal.reclaim_bytes),
            removal.remove.len()
        )],
    )?;
    println!(
        "{}",
        paint(Color::Green, &format!("✔ 已写出 plan: {}", a.out.display()))
    );
    println!("下一步:  git pomona clean {}", a.out.display());
    Ok(())
}
