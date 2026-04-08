//! oryx-bench CLI entry point.

use std::process::ExitCode;

use oryx_bench::cli;

fn main() -> ExitCode {
    // Respect RUST_LOG / ORYX_BENCH_LOG; default to info.
    let env_filter = tracing_subscriber::EnvFilter::try_from_env("ORYX_BENCH_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .without_time()
        .with_target(false)
        .try_init();

    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}
