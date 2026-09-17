use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::cli::CleanArgs;
use crate::git::ProcessGit;
use crate::inventory::{KeepRefs, build, expand_globs};
use crate::mirror::{Source, build as build_mirror};
use crate::model::SortBy;
use crate::plan::Plan;
use crate::prompt::{Answer, Prompt, TerminalPrompt};
use crate::publish::{PUSH_INFO_VERSION, PushInfo, snapshot_tips, verify};
use crate::removal::plan_removal;
use crate::rewrite::{Engine, filter_repo, rewriter_for};
use crate::size::human;
use crate::term::{Color, banner, paint};

fn dir_size(p: &Path) -> u64 {
    fn walk(p: &Path, acc: &mut u64) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                walk(&e.path(), acc);
            } else {
                *acc += m.len();
            }
        }
    }
    let mut n = 0;
    walk(p, &mut n);
    n
}

pub fn run(a: CleanArgs) -> Result<()> {
    let plan = Plan::load(&a.plan)?;
    if a.engine == Engine::FilterRepo && !filter_repo::available() {
        return Err(anyhow!(
            "git-filter-repo 未安装（pip3.12 install git-filter-repo），或去掉 --engine 用默认原生引擎"
        ));
    }
    let g = ProcessGit;
    let rewriter = rewriter_for(a.engine);
    let src = Source::resolve(
        plan.source.url.as_deref(),
        plan.source.local.as_deref().map(Path::new),
    )?;
    banner(&[
        " ② 执行清理".into(),
        format!(" 来源: {}", src.describe()),
        format!(" 选中路径: {} 个", plan.paths.len()),
        format!(" 引擎: {}", rewriter.name()),
    ]);
    println!();
    let workdir: PathBuf = match a.workdir {
        Some(w) => w,
        None => tempfile::Builder::new()
            .prefix("pomona-")
            .tempdir()?
            .keep()
            .join("mirror.git"),
    };

    println!(
        "{}",
        paint(
            Color::Yellow,
            &format!("[1/4] 建镜像 -> {}", workdir.display())
        )
    );
    let branches = build_mirror(&g, &src, &workdir)?;
    println!(
        "{}",
        paint(Color::Cyan, &format!("  分支数: {}", branches.len()))
    );
    println!();

    println!(
        "{}",
        paint(
            Color::Yellow,
            "[2/4] 计算 KEEP（各分支 tip）与 REMOVE（选中路径的历史版本）"
        )
    );
    let branch_refs = expand_globs(&g, &workdir, &["refs/heads/".to_string()])?;
    let extra_refs = expand_globs(&g, &workdir, &plan.policy.keep_refs)?;
    let mut all_keep = branch_refs.clone();
    all_keep.extend(extra_refs.iter().cloned());
    let inv = build(
        &g,
        &workdir,
        &KeepRefs {
            branch_refs,
            branch_prefix: "refs/heads/".into(),
            extra_refs,
        },
        SortBy::Max,
    )?;
    let removal = plan_removal(&inv, &plan.path_list());
    println!(
        "  待删历史 blob: {}  tip 在用(保留): {}  预计回收: {}",
        paint(Color::Red, &removal.remove.len().to_string()),
        paint(Color::Green, &removal.kept.to_string()),
        paint(Color::Green, &human(removal.reclaim_bytes))
    );
    println!();
    if removal.remove.is_empty() {
        println!("没有可删的历史版本，退出。");
        return Ok(());
    }
    if !a.assume_yes {
        let q = paint(Color::Yellow, "将重写镜像副本历史（不动原仓库）。继续?");
        if TerminalPrompt.confirm(&q, false)? != Answer::Yes {
            println!("已取消。");
            return Ok(());
        }
    }

    let info = PushInfo {
        version: PUSH_INFO_VERSION,
        url: src.url().map(str::to_string),
        branches: branches.clone(),
        tips: snapshot_tips(&g, &workdir, &all_keep)?,
        removed: removal.remove.iter().map(|o| o.to_string()).collect(),
        reclaim_bytes: removal.reclaim_bytes,
    };
    let info_path = PushInfo::path_for(&workdir);
    info.save(&info_path)?;

    println!(
        "{}",
        paint(
            Color::Yellow,
            &format!("[3/4] 重写历史（{}）", rewriter.name())
        )
    );
    let before = dir_size(&workdir);
    let report = rewriter.strip_blobs(&workdir, &removal.remove)?;
    println!("  改写文件变更条目: {}", report.rewritten_entries);
    println!();

    println!(
        "{}",
        paint(
            Color::Yellow,
            "[4/4] 自检：各 keep ref 树不变、待删 blob 不可达"
        )
    );
    verify(&g, &workdir, &info)?;
    println!("  {}", paint(Color::Green, "通过"));
    let after = dir_size(&workdir);
    println!();
    println!("{}", paint(Color::Cyan, "========== 清理完成 =========="));
    println!("  镜像目录:   {}", workdir.display());
    println!(
        "  体积:       {} -> {}",
        paint(Color::Red, &human(before)),
        paint(Color::Green, &human(after))
    );
    println!(
        "  删除历史 blob: {}   分支: {}",
        removal.remove.len(),
        branches.len()
    );
    println!("  推送信息:   {}", info_path.display());
    println!();
    println!("下一步:  git pomona push {}", workdir.display());
    Ok(())
}
