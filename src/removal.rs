use std::collections::BTreeSet;

use crate::model::{Inventory, Oid, Removal};

/// 选中路径的全部历史 blob 减去 keep_blobs。analyze 预览与 clean 执行共用同一函数。
pub fn plan_removal(inv: &Inventory, selected: &[String]) -> Removal {
    let sel: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    let mut remove = BTreeSet::new();
    let mut kept: BTreeSet<&Oid> = BTreeSet::new();
    let mut reclaim_bytes = 0u64;
    for o in &inv.objects {
        if !sel.contains(o.path.as_str()) {
            continue;
        }
        if inv.keep_blobs.contains(&o.oid) {
            kept.insert(&o.oid);
        } else if remove.insert(o.oid.clone()) {
            reclaim_bytes += o.size;
        }
    }
    Removal {
        remove,
        kept: kept.len(),
        reclaim_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn inv() -> Inventory {
        let objects = vec![
            ObjectRec {
                oid: Oid::new("a1"),
                size: 100,
                path: "app.tar.gz".into(),
            },
            ObjectRec {
                oid: Oid::new("a2"),
                size: 200,
                path: "app.tar.gz".into(),
            },
            ObjectRec {
                oid: Oid::new("a3"),
                size: 300,
                path: "app.tar.gz".into(),
            },
            ObjectRec {
                oid: Oid::new("s1"),
                size: 10,
                path: "src.go".into(),
            },
        ];
        let mut inv = Inventory::from_objects(objects, SortBy::Max);
        inv.keep_blobs.insert(Oid::new("a3"));
        inv.keep_blobs.insert(Oid::new("s1"));
        inv.keep_paths.insert("app.tar.gz".into());
        inv.keep_paths.insert("src.go".into());
        inv
    }

    #[test]
    fn removes_only_non_tip_blobs_of_selected_paths() {
        let r = plan_removal(&inv(), &["app.tar.gz".to_string()]);
        assert_eq!(
            r.remove.iter().map(|o| o.as_str()).collect::<Vec<_>>(),
            ["a1", "a2"]
        );
        assert_eq!(r.kept, 1);
        assert_eq!(r.reclaim_bytes, 300);
    }

    #[test]
    fn unselected_paths_untouched() {
        let r = plan_removal(&inv(), &["src.go".to_string()]);
        assert!(r.remove.is_empty());
        assert_eq!(r.kept, 1);
    }

    #[test]
    fn stats_aggregate_per_path() {
        let i = inv();
        let app = i.stats.iter().find(|s| s.path == "app.tar.gz").unwrap();
        assert_eq!((app.max, app.cum, app.versions), (300, 600, 3));
        assert_eq!(i.stats[0].path, "app.tar.gz");
        let mut byc = inv();
        byc.sort(SortBy::Cum);
        assert_eq!(byc.stats[0].path, "app.tar.gz");
    }

    #[test]
    fn orphan_detection() {
        let mut i = inv();
        i.keep_paths.remove("src.go");
        assert!(i.is_orphan("src.go"));
        assert_eq!(i.orphans(), vec!["src.go".to_string()]);
    }

    #[test]
    fn reason_roundtrip() {
        for r in [
            Reason::Orphan,
            Reason::Size,
            Reason::Suffix,
            Reason::Regex,
            Reason::Manual,
        ] {
            assert_eq!(r.to_string().parse::<Reason>().unwrap(), r);
        }
        assert!("nope".parse::<Reason>().is_err());
    }
}
