//! A gdformat-compatible command: gdtoolkit 4.5.0's `gdformat`, minus `--diff`.

use std::io::{self, ErrorKind};
use std::process::ExitCode;

use gdformat::cli::{Io, run};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("gdformat: cannot read the working directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (mut stdin, mut stdout, mut stderr) = (io::stdin(), io::stdout(), io::stderr());
    let mut io = Io {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
    };
    match run(&args, &cwd, &mut io) {
        Ok(status) => ExitCode::from(status as u8),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("gdformat: cannot write the output: {error}");
            ExitCode::FAILURE
        }
    }
}
