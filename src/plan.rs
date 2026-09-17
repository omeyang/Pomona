use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

pub const PLAN_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    pub remote: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanPolicy {
    pub min_size: u64,
    #[serde(default)]
    pub keep_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanPath {
    pub path: String,
    pub reason: String,
}

/// analyze 的产物、clean 的输入
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub version: u32,
    pub generated_by: String,
    pub source: PlanSource,
    pub policy: PlanPolicy,
    #[serde(default)]
    pub paths: Vec<PlanPath>,
}

impl Plan {
    /// 开头若干 `# ` 注释行 + TOML 正文
    pub fn render(&self, notes: &[String]) -> String {
        let mut s =
            String::from("# Pomona plan（由 git pomona analyze 生成，供 git pomona clean 使用）\n");
        for n in notes {
            s.push_str("# ");
            s.push_str(n);
            s.push('\n');
        }
        s.push('\n');
        s.push_str(&toml::to_string_pretty(self).expect("plan 可序列化"));
        s
    }

    pub fn parse(text: &str) -> Result<Plan> {
        let p: Plan = toml::from_str(text).context("plan 解析失败")?;
        if p.version != PLAN_VERSION {
            return Err(anyhow!(
                "plan version {} 不受支持（当前只支持 {}）",
                p.version,
                PLAN_VERSION
            ));
        }
        if p.paths.is_empty() {
            return Err(anyhow!("plan 里没有要清理的路径"));
        }
        Ok(p)
    }

    pub fn load(path: &Path) -> Result<Plan> {
        Plan::parse(
            &std::fs::read_to_string(path)
                .with_context(|| format!("读取 plan {}", path.display()))?,
        )
    }

    pub fn save(&self, path: &Path, notes: &[String]) -> Result<()> {
        std::fs::write(path, self.render(notes))
            .with_context(|| format!("写 plan {}", path.display()))
    }

    pub fn path_list(&self) -> Vec<String> {
        self.paths.iter().map(|p| p.path.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Plan {
        Plan {
            version: PLAN_VERSION,
            generated_by: "pomona test".into(),
            source: PlanSource {
                url: Some("git@h:g/r.git".into()),
                local: Some("/tmp/r".into()),
                remote: "origin".into(),
            },
            policy: PlanPolicy {
                min_size: 52428800,
                keep_refs: vec!["refs/tags/*".into(), "refs/heads/keep me".into()],
            },
            paths: vec![
                PlanPath {
                    path: "app.tar.gz".into(),
                    reason: "size".into(),
                },
                PlanPath {
                    path: "pkg dir/x.zip".into(),
                    reason: "suffix".into(),
                },
            ],
        }
    }

    #[test]
    fn roundtrip_keeps_everything() {
        let text = sample().render(&["预计可回收: 1.2GB".into()]);
        assert!(text.starts_with("# Pomona plan"));
        assert!(text.contains("# 预计可回收: 1.2GB"));
        let back = Plan::parse(&text).unwrap();
        assert_eq!(back.policy.keep_refs, sample().policy.keep_refs);
        assert_eq!(back.path_list(), vec!["app.tar.gz", "pkg dir/x.zip"]);
        assert_eq!(back.source.url.as_deref(), Some("git@h:g/r.git"));
        assert_eq!(back, sample());
    }

    #[test]
    fn rejects_bad_version_and_empty_paths() {
        let mut p = sample();
        p.version = 2;
        assert!(Plan::parse(&p.render(&[])).is_err());
        let mut p = sample();
        p.paths.clear();
        assert!(Plan::parse(&p.render(&[])).is_err());
    }

    #[test]
    fn missing_url_is_none() {
        let mut p = sample();
        p.source.url = None;
        let back = Plan::parse(&p.render(&[])).unwrap();
        assert_eq!(back.source.url, None);
    }
}
