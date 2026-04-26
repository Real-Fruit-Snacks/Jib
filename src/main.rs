use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let rc = jib::cli::run(&argv);
    // Mirror POSIX: clamp to u8.
    ExitCode::from((rc & 0xff) as u8)
}
