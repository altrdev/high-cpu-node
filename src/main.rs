use std::process::ExitCode;

fn main() -> ExitCode {
    torchio::cli::run(&std::env::args().skip(1).collect::<Vec<_>>())
}
