mod common;
use std::collections::BTreeSet;

use common::*;
use pomona::model::Oid;
use pomona::rewrite::{Engine, rewriter_for};

fn mirror_of(f: &Fixture) -> std::path::PathBuf {
    let m = f.dir.path().join("mirror.git");
    git(
        f.dir.path(),
        &[
            "clone",
            "-q",
            "--mirror",
            f.upstream.to_str().unwrap(),
            m.to_str().unwrap(),
        ],
    );
    m
}

const BRANCHES: [&str; 4] = ["main", "release-1", "release-2", "release-3"];

fn trees(m: &std::path::Path) -> Vec<String> {
    BRANCHES
        .iter()
        .map(|b| git(m, &["rev-parse", &format!("{b}^{{tree}}")]))
        .collect()
}

fn run_engine(engine: Engine) {
    let f = release_fixture();
    let m = mirror_of(&f);
    // 附注标签 + 含空格路径 + 合并提交，覆盖流里的特殊情况
    git(
        &m,
        &["tag", "-a", "keep-tag", "-m", "annotated", "release-1"],
    );
    let tips: Vec<String> = ["release-1", "release-2", "release-3"]
        .iter()
        .map(|b| blob_of(&m, b, "app.tar.gz"))
        .collect();
    let trees_before = trees(&m);
    let root = git(&m, &["rev-list", "--max-parents=0", "main"]);
    let remove: BTreeSet<Oid> = history_blobs(&m, "app.tar.gz")
        .into_iter()
        .filter(|o| !tips.contains(o))
        .map(Oid::new)
        .collect();
    assert_eq!(remove.len(), 6);

    let report = rewriter_for(engine).strip_blobs(&m, &remove).unwrap();
    if engine == Engine::Native {
        assert_eq!(report.rewritten_entries, 6);
    }
    assert_eq!(trees_before, trees(&m), "tip 树必须不变");
    assert_eq!(
        git(&m, &["rev-list", "--max-parents=0", "main"]),
        root,
        "未受影响的根提交 SHA 不变"
    );
    for o in &remove {
        assert!(!object_exists(&m, o.as_str()), "{o} 应已回收");
    }
    for t in &tips {
        assert!(object_exists(&m, t));
    }
    assert_eq!(
        git(&m, &["cat-file", "-t", "keep-tag"]),
        "tag",
        "附注标签保留"
    );
    git(&m, &["fsck", "--no-dangling"]);
}

#[test]
fn native_engine_strips_history_blobs() {
    run_engine(Engine::Native);
}

#[test]
fn filter_repo_engine_strips_history_blobs() {
    if !pomona::rewrite::filter_repo::available() {
        eprintln!("git-filter-repo 未安装，跳过");
        return;
    }
    run_engine(Engine::FilterRepo);
}
