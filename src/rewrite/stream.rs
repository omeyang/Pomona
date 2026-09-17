use std::collections::BTreeSet;
use std::io::{BufRead, Read, Write};

use anyhow::{Context, Result, anyhow};

use crate::model::Oid;

/// 逐行透传 fast-export 流；`data <n>` 块按字节原样拷贝（其中可能出现长得像指令的行）；
/// 命中 remove 的 `M <mode> <sha> <path>` 改成 `D <path>`（path 含引号时原样透传）。
/// 返回改写的行数。
pub fn filter_stream<R: BufRead, W: Write>(
    mut input: R,
    remove: &BTreeSet<Oid>,
    mut out: W,
) -> Result<usize> {
    let mut line = Vec::new();
    let mut rewritten = 0usize;
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if let Some(rest) = line.strip_prefix(b"data ") {
            out.write_all(&line)?;
            let spec = std::str::from_utf8(rest)
                .context("data 行不是 UTF-8")?
                .trim_end();
            if let Some(delim) = spec.strip_prefix("<<") {
                let delim = delim.as_bytes().to_vec();
                loop {
                    line.clear();
                    if input.read_until(b'\n', &mut line)? == 0 {
                        return Err(anyhow!("fast-export 流在分隔符 data 块中截断"));
                    }
                    out.write_all(&line)?;
                    if line.strip_suffix(b"\n").unwrap_or(&line) == delim.as_slice() {
                        break;
                    }
                }
            } else {
                let n: u64 = spec
                    .parse()
                    .with_context(|| format!("data 长度不合法: {spec}"))?;
                let copied = std::io::copy(&mut (&mut input).take(n), &mut out)?;
                if copied != n {
                    return Err(anyhow!(
                        "fast-export 流在 data 块中截断（期望 {n} 字节，实得 {copied}）"
                    ));
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix(b"M ") {
            // rest = "<mode> <dataref> <path>\n"
            let mut parts = rest.splitn(3, |b| *b == b' ');
            if let (Some(_mode), Some(dataref), Some(path)) =
                (parts.next(), parts.next(), parts.next())
            {
                let dataref = std::str::from_utf8(dataref).unwrap_or("");
                if !dataref.starts_with(':') && remove.contains(&Oid::new(dataref)) {
                    out.write_all(b"D ")?;
                    out.write_all(path)?;
                    rewritten += 1;
                    continue;
                }
            }
        }
        out.write_all(&line)?;
    }
    out.flush()?;
    Ok(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[u8], remove: &[&str]) -> (Vec<u8>, usize) {
        let set: BTreeSet<Oid> = remove.iter().map(|s| Oid::new(*s)).collect();
        let mut out = Vec::new();
        let n = filter_stream(std::io::Cursor::new(input), &set, &mut out).unwrap();
        (out, n)
    }

    #[test]
    fn rewrites_matching_m_lines_to_d() {
        let input = b"commit refs/heads/main\nmark :1\ncommitter t <t@t.io> 1 +0000\ndata 3\nmsg\nM 100644 aaaa app.tar.gz\nM 100644 bbbb \"pkg dir/app.tar.gz\"\nM 100644 cccc keep.go\n\n";
        let (out, n) = run(input, &["aaaa", "bbbb"]);
        assert_eq!(n, 2);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\nD app.tar.gz\n"));
        assert!(s.contains("\nD \"pkg dir/app.tar.gz\"\n"));
        assert!(s.contains("\nM 100644 cccc keep.go\n"));
    }

    #[test]
    fn data_block_is_copied_verbatim_even_if_it_looks_like_commands() {
        let body = b"M 100644 aaaa app.tar.gz\nD nope\n";
        let mut input = Vec::new();
        input.extend_from_slice(format!("commit refs/heads/x\ndata {}\n", body.len()).as_bytes());
        input.extend_from_slice(body);
        input.extend_from_slice(b"\nM 100644 aaaa app.tar.gz\n");
        let (out, n) = run(&input, &["aaaa"]);
        assert_eq!(n, 1);
        let s = String::from_utf8(out).unwrap();
        let expected = format!(
            "data {}\nM 100644 aaaa app.tar.gz\nD nope\n\nD app.tar.gz\n",
            body.len()
        );
        assert!(s.contains(&expected), "{s}");
    }

    #[test]
    fn truncated_data_block_is_an_error() {
        let set: BTreeSet<Oid> = BTreeSet::new();
        let mut out = Vec::new();
        assert!(filter_stream(std::io::Cursor::new(b"data 10\nshort"), &set, &mut out).is_err());
    }

    #[test]
    fn marks_and_gitlinks_untouched() {
        let input = b"M 100644 :7 x\nM 160000 aaaa sub\nreset refs/heads/y\nfrom :3\ndone\n";
        let (out, n) = run(input, &["aaaa"]);
        assert_eq!(n, 1);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("M 100644 :7 x\n"));
        assert!(s.contains("D sub\n"));
        assert!(s.ends_with("done\n"));
    }

    #[test]
    fn delimited_data_block() {
        let input = b"data <<EOT\nM 100644 aaaa fake\nEOT\n\nM 100644 aaaa real\n";
        let (out, n) = run(input, &["aaaa"]);
        assert_eq!(n, 1);
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("M 100644 aaaa fake\nEOT\n\nD real\n")
        );
    }
}
