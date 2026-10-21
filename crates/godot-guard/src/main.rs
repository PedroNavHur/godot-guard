fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = godot_guard::schedule::stop_on_signals() {
        eprintln!("godot-guard: {error}");
        std::process::exit(1);
    }
    // sys.exit(status) exits with the status's low byte, so a signal's negative status becomes 128+.
    std::process::exit(godot_guard::cli::main(&argv) & 0xFF);
}
