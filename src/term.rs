use std::io::IsTerminal;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Yellow,
    Cyan,
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

pub fn paint(c: Color, s: &str) -> String {
    if !enabled() {
        return s.to_string();
    }
    let code = match c {
        Color::Red => "0;31",
        Color::Green => "0;32",
        Color::Yellow => "1;33",
        Color::Cyan => "0;36",
    };
    format!("\x1b[{code}m{s}\x1b[0m")
}

pub fn banner(lines: &[String]) {
    let bar = paint(Color::Cyan, "========================================");
    println!("{bar}");
    for l in lines {
        println!("{}", paint(Color::Cyan, l));
    }
    println!("{bar}");
}
