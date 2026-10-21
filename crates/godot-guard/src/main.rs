fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // sys.exit(status) exits with the status's low byte, so a signal's negative status becomes 128+.
    std::process::exit(godot_guard::cli::main(&argv) & 0xFF);
}
