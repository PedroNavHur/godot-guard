//! A `gdlint`-compatible command over `gdscript::parse`.

use std::io::{self, ErrorKind, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("gdlint: cannot read the working directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match lint(&argv, &cwd) {
        Ok(status) => ExitCode::from(status as u8),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("gdlint: cannot write the output: {error}");
            ExitCode::FAILURE
        }
    }
}

fn lint(argv: &[String], cwd: &std::path::Path) -> io::Result<i32> {
    let (mut out, mut err) = (io::stdout().lock(), io::stderr().lock());
    let status = gdlint::cli::run(argv, cwd, &gdscript::parse, &mut out, &mut err)?;
    out.flush()?;
    Ok(status)
}
