//! A `gdlint`-compatible command over `gdscript::parse`.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cwd = std::env::current_dir().expect("the working directory");
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (mut out, mut err) = (std::io::stdout().lock(), std::io::stderr().lock());
    let status =
        gdlint::cli::run(&argv, &cwd, &gdscript::parse, &mut out, &mut err).expect("write output");
    out.flush().expect("flush stdout");
    ExitCode::from(status as u8)
}
