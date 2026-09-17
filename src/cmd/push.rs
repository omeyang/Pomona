use anyhow::{Result, anyhow};

use crate::cli::PushArgs;
use crate::git::{self, Git, ProcessGit};
use crate::hooks::{HookOutcome, run_hook};
use crate::profile::config_dir;
use crate::publish::{PushInfo, largest_files, push_args, verify};
use crate::size::human;
use crate::term::{Color, banner, paint};

pub fn run(a: PushArgs) -> Result<()> {
    let g = ProcessGit;
    if !a.mirror.is_dir() || !git::is_repo(&g, &a.mirror) {
        return Err(anyhow!(
            "镜像目录不存在或不是 git 仓库: {}",
            a.mirror.display()
        ));
    }
    let info_path = PushInfo::path_for(&a.mirror);
    let info = if info_path.is_file() {
        Some(PushInfo::load(&info_path)?)
    } else {
        None
    };
    let url = a
        .url
        .clone()
        .or_else(|| info.as_ref().and_then(|i| i.url.clone()))
        .ok_or_else(|| anyhow!("未提供 --url，且 {} 里无 URL", info_path.display()))?;
    banner(&[
        " ③ 清理后收尾".into(),
        format!(" 镜像: {}", a.mirror.display()),
        format!(" 远程: {url}"),
    ]);
    println!();

    match &info {
        Some(i) => {
            verify(&g, &a.mirror, i)?;
            println!(
                "{}",
                paint(
                    Color::Green,
                    &format!(
                        "[校验] {} 个 ref 的 tip 树与清理前一致，{} 个待删 blob 均已不可达",
                        i.tips.len(),
                        i.removed.len()
                    )
                )
            );
        }
        None => println!(
            "{}",
            paint(
                Color::Yellow,
                &format!(
                    "[校验] 未找到 {}，跳过自动校验（只做人工列表）",
                    info_path.display()
                )
            )
        ),
    }
    println!(
        "{}",
        paint(
            Color::Yellow,
            "[校验] 各分支 tip 里最大的文件（确认包仍在、且只剩一个）:"
        )
    );
    for r in git::for_each_ref(&g, &a.mirror, &["refs/heads/"])? {
        println!(
            "  {}",
            paint(Color::Cyan, r.trim_start_matches("refs/heads/"))
        );
        for (sz, p) in largest_files(&g, &a.mirror, &r, 3)? {
            println!("      {}  {}", human(sz), p);
        }
    }
    println!();

    let args = push_args(&url, a.mirror_mode);
    if !a.yes {
        println!(
            "{}",
            paint(Color::Yellow, "[dry-run] 将执行（加 --yes 才真正推送）:")
        );
        println!("  git -C {} {}", a.mirror.display(), args.join(" "));
        println!();
        print_resync_guide();
        return Ok(());
    }
    let hooks_dir = a
        .hooks_dir
        .clone()
        .unwrap_or_else(|| config_dir().join("hooks"));
    let env = hook_env(&a, &url, info.as_ref());
    if let HookOutcome::Ran = run_hook(&hooks_dir, "pre-push", &env)? {
        println!("{}", paint(Color::Cyan, "[hook] pre-push 已执行"));
    }
    println!(
        "{}",
        paint(Color::Red, "[执行] 强推回远程（改写共享历史）...")
    );
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    g.run_inherit(Some(&a.mirror), &argv)?;
    println!("{}", paint(Color::Green, "✔ 已推送"));
    if let HookOutcome::Ran = run_hook(&hooks_dir, "post-push", &env)? {
        println!("{}", paint(Color::Cyan, "[hook] post-push 已执行"));
    }
    println!();
    print_resync_guide();
    Ok(())
}

fn hook_env(a: &PushArgs, url: &str, info: Option<&PushInfo>) -> Vec<(&'static str, String)> {
    let mut v = vec![
        ("POMONA_MIRROR", a.mirror.to_string_lossy().to_string()),
        ("POMONA_URL", url.to_string()),
    ];
    if let Some(i) = info {
        v.push(("POMONA_REMOVED_BLOBS", i.removed.len().to_string()));
        v.push(("POMONA_RECLAIMED_BYTES", i.reclaim_bytes.to_string()));
        v.push(("POMONA_BRANCHES", i.branches.join(",")));
    }
    v
}

fn print_resync_guide() {
    println!(
        "{}",
        paint(
            Color::Yellow,
            "推送后，所有已有克隆都与新历史不一致，必须重新同步:"
        )
    );
    println!("  • 你本地的旧克隆（包括 analyze 用的那个）: 最简单是删掉重新 git clone；");
    println!(
        "    或逐分支: git fetch origin && git reset --hard origin/<分支> && git reflog expire --all --expire=now && git gc --prune=now"
    );
    println!("  • 协作者: 一律重新 clone（他们的旧分支基于被改写的历史，不能直接 pull）");
    println!(
        "  • 远程服务器磁盘: 强推只是让旧对象不可达；自建 GitLab/Gitea 需服务端 git gc / housekeeping 才会真正回收"
    );
}
