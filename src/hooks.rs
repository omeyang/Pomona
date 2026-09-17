use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

pub enum HookOutcome {
    Missing,
    Ran,
}

/// 执行 `<dir>/<name>`（存在即执行），通过环境变量传参；非 0 退出报错
pub fn run_hook(dir: &Path, name: &str, env: &[(&str, String)]) -> Result<HookOutcome> {
    let hook = dir.join(name);
    if !hook.is_file() {
        return Ok(HookOutcome::Missing);
    }
    let mut c = Command::new(&hook);
    for (k, v) in env {
        c.env(k, v);
    }
    let st = c
        .status()
        .with_context(|| format!("执行 hook {}", hook.display()))?;
    if st.success() {
        Ok(HookOutcome::Ran)
    } else {
        Err(anyhow!(
            "hook {} 退出码 {}",
            hook.display(),
            st.code().unwrap_or(-1)
        ))
    }
}
