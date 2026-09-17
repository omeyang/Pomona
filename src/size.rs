use anyhow::{Result, anyhow};

/// "50M" / "100K" / "2G" / "123" / "50MB" -> 字节数
pub fn parse_size(s: &str) -> Result<u64> {
    let t = s.trim();
    let t = t.strip_suffix(['B', 'b']).unwrap_or(t);
    let split = t
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i);
    let (num, unit) = match split {
        Some(i) => t.split_at(i),
        None => (t, ""),
    };
    if num.is_empty() {
        return Err(anyhow!(
            "大小格式不对: {s:?}（示例 50M / 100K / 2G / 纯字节）"
        ));
    }
    let n: u64 = num.parse().map_err(|_| anyhow!("大小格式不对: {s:?}"))?;
    let mult: u64 = match unit {
        "" => 1,
        "K" | "k" => 1 << 10,
        "M" | "m" => 1 << 20,
        "G" | "g" => 1 << 30,
        _ => return Err(anyhow!("大小单位不对: {s:?}（支持 K / M / G）")),
    };
    n.checked_mul(mult)
        .ok_or_else(|| anyhow!("大小溢出: {s:?}"))
}

/// 字节数 -> 人类可读（1.5GB / 12.0MB / 3.2KB / 12B）
pub fn human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = bytes as f64;
    if f >= GB {
        format!("{:.1}GB", f / GB)
    } else if f >= MB {
        format!("{:.1}MB", f / MB)
    } else if f >= KB {
        format!("{:.1}KB", f / KB)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_units() {
        assert_eq!(parse_size("50M").unwrap(), 50 * 1024 * 1024);
        assert_eq!(parse_size("100K").unwrap(), 100 * 1024);
        assert_eq!(parse_size("2G").unwrap(), 2 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("12345").unwrap(), 12345);
        assert_eq!(parse_size("50MB").unwrap(), 50 * 1024 * 1024);
        assert_eq!(parse_size(" 3k ").unwrap(), 3 * 1024);
    }

    #[test]
    fn rejects_bad_input() {
        for s in ["", "M", "1T", "abc", "-5M", "1.5G"] {
            assert!(parse_size(s).is_err(), "{s} 应报错");
        }
    }

    #[test]
    fn human_readable() {
        assert_eq!(human(0), "0B");
        assert_eq!(human(1023), "1023B");
        assert_eq!(human(1536), "1.5KB");
        assert_eq!(human(300 * 1024 * 1024), "300.0MB");
        assert_eq!(human(3 * 1024 * 1024 * 1024 / 2), "1.5GB");
    }
}
