//! A gdformat-compatible command: gdtoolkit 4.5.0's `gdformat`, minus `--diff`.

use gdformat::cli::{Io, run};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().expect("working directory");
    let (mut stdin, mut stdout, mut stderr) =
        (std::io::stdin(), std::io::stdout(), std::io::stderr());
    let mut io = Io {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
    };
    std::process::exit(run(&args, &cwd, &mut io));
}
