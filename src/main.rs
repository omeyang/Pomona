use clap::Parser;
use pomona::cli::{Cli, Cmd};
use pomona::term::{Color, paint};

fn main() {
    let cli = Cli::parse();
    let r = match cli.cmd {
        Cmd::Analyze(a) => pomona::cmd::analyze::run(a),
        Cmd::Clean(a) => pomona::cmd::clean::run(a),
        Cmd::Push(a) => pomona::cmd::push::run(a),
    };
    if let Err(e) = r {
        eprintln!("{}", paint(Color::Red, &format!("错误: {e:#}")));
        std::process::exit(1);
    }
}
