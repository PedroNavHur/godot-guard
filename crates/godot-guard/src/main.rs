use std::ffi::OsString;

/// The arguments after the program name. One that is not UTF-8 cannot be a path or option this port reads,
/// so it is a usage error, status 2 as argparse's.
fn arguments() -> Result<Vec<String>, OsString> {
    std::env::args_os()
        .skip(1)
        .map(OsString::into_string)
        .collect()
}

fn main() {
    let argv = arguments().unwrap_or_else(|argument| {
        eprintln!("godot-guard: error: argument {argument:?} is not valid UTF-8");
        std::process::exit(2);
    });
    if let Err(error) = godot_guard::schedule::stop_on_signals() {
        eprintln!("godot-guard: {error}");
        std::process::exit(1);
    }
    // sys.exit(status) exits with the status's low byte, so a signal's negative status becomes 128+.
    std::process::exit(godot_guard::cli::main(&argv) & 0xFF);
}
