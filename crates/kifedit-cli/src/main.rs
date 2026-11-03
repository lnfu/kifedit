use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use kifedit_syntax::parser::parse;
use kifedit_syntax::{FormatOptions, format_file};

#[derive(Parser)]
#[command(name = "kifedit", version, about = "SUO-KIF ontology editor")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Format SUO-KIF files.
    Fmt {
        /// Do not write the files. Show the files that are not formatted.
        #[arg(long)]
        check: bool,

        /// The files to format.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Fmt { check, files } => fmt(&files, check),
    }
}

// 結束碼與 rustfmt --check 相同：0 全部已格式化，1 有檔案需要格式化，2 讀寫失敗
fn fmt(files: &[PathBuf], check: bool) -> ExitCode {
    let options = FormatOptions::default();
    let mut unformatted = 0;
    let mut failed = false;

    for path in files {
        let src = match std::fs::read_to_string(path) {
            Ok(src) => src,
            Err(e) => {
                eprintln!("{}: Cannot read the file: {e}", path.display());
                failed = true;
                continue;
            }
        };
        let out = format_file(&parse(&src).syntax(), &options);
        if out == src {
            continue;
        }
        if check {
            println!("{}: The file is not formatted.", path.display());
            unformatted += 1;
        } else if let Err(e) = std::fs::write(path, &out) {
            eprintln!("{}: Cannot write the file: {e}", path.display());
            failed = true;
        }
    }

    if unformatted > 0 {
        eprintln!(
            "Files that are not formatted: {unformatted} of {}. Use `kifedit fmt` to format them.",
            files.len()
        );
    }
    if failed {
        ExitCode::from(2)
    } else if unformatted > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
