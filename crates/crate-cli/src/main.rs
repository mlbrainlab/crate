use std::process::ExitCode;

const USAGE: &str = "\
Usage: crate <command> [args]

Commands (coming in M1):
  l <archive>    list the contents of an archive
  x <archive>    extract an archive

Options:
  -h, --help     show this help
  -V, --version  show the version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("crate {}", crate_core::version());
            ExitCode::SUCCESS
        }
        Some(cmd @ ("l" | "x")) => {
            eprintln!("crate: `{cmd}` is not implemented yet (milestone M1)");
            ExitCode::FAILURE
        }
        Some(other) => {
            eprintln!("crate: unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
