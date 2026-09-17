use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::rewrite::Engine;

#[derive(Parser)]
#[command(
    name = "git-pomona",
    version,
    about = "Pomona：按分支 tip 保留包、清除历史旧版本的 git 瘦身工具",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// ① 只读分析本地仓库，选定要清理的路径，产出 plan
    Analyze(AnalyzeArgs),
    /// ② 按 plan 建镜像并重写历史（不动原仓库，不推送）
    Clean(CleanArgs),
    /// ③ 校验镜像并强推回远程（默认 dry-run）
    Push(PushArgs),
}

#[derive(clap::Args)]
pub struct AnalyzeArgs {
    /// 本地仓库目录（普通克隆即可，只检出一个分支也没关系）
    pub repo: PathBuf,
    /// 读哪个 remote 的 URL
    #[arg(long, default_value = "origin")]
    pub remote: String,
    /// 交互模式逐个审阅的前 N 大（未传时运行时询问，默认 30）
    #[arg(short = 'n', long)]
    pub top: Option<usize>,
    /// 前 N 大的排序口径：max=单版本最大，cum=累计体积
    #[arg(long, default_value = "max", value_parser = ["max", "cum"])]
    pub sort_by: String,
    /// 交互模式审阅全部命中项（不截断到前 N）
    #[arg(long)]
    pub all: bool,
    /// 额外选中「所有分支 tip 都不再引用」的孤儿路径（无视大小，自动标记）
    #[arg(long)]
    pub orphans: bool,
    /// 按大小识别包的阈值：50M / 100K / 2G / 字节（默认取 profile，内置 50M）
    #[arg(long)]
    pub min_size: Option<String>,
    /// 非交互：选中「单版本 ≥ 阈值 或 命中包扩展名」的全部路径
    #[arg(long)]
    pub auto_detect: bool,
    /// 非交互：额外选中匹配正则的路径（可重复）
    #[arg(long = "auto-select", value_name = "正则")]
    pub auto_select: Vec<String>,
    /// 额外纳入「保留 tip」的 ref glob（可重复；默认所有分支；例 refs/tags/*）
    #[arg(long = "keep-refs", value_name = "GLOB")]
    pub keep_refs: Vec<String>,
    /// 识别规则配置文件（默认 $XDG_CONFIG_HOME/pomona/profile.toml）
    #[arg(long)]
    pub profile: Option<PathBuf>,
    /// plan 输出路径
    #[arg(short, long, default_value = "pomona-plan.toml")]
    pub out: PathBuf,
}

#[derive(clap::Args)]
pub struct CleanArgs {
    /// analyze 产出的 plan 文件
    pub plan: PathBuf,
    /// 镜像克隆目标目录（默认临时目录）
    #[arg(long)]
    pub workdir: Option<PathBuf>,
    /// 重写引擎
    #[arg(long, value_enum, default_value_t = Engine::Native)]
    pub engine: Engine,
    /// 跳过重写前的确认
    #[arg(long)]
    pub assume_yes: bool,
}

#[derive(clap::Args)]
pub struct PushArgs {
    /// clean 产出的镜像目录
    pub mirror: PathBuf,
    /// 远程地址（默认从 <镜像>.pomona-push.toml 读）
    #[arg(long)]
    pub url: Option<String>,
    /// 真正执行强推（默认只 dry-run）
    #[arg(long)]
    pub yes: bool,
    /// 用 git push --mirror（会删除远程上本地没有的 ref）；默认只推 heads + tags
    #[arg(long = "mirror")]
    pub mirror_mode: bool,
    /// hooks 所在目录（默认 $XDG_CONFIG_HOME/pomona/hooks）
    #[arg(long)]
    pub hooks_dir: Option<PathBuf>,
}
