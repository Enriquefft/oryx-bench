//! One module per CLI subcommand. The `cli.rs` dispatch layer is thin;
//! each command is a self-contained function that takes its clap-derived
//! `Args` and returns an `ExitCode`.

pub mod attach;
pub mod build;
pub mod detach;
pub mod diff;
pub mod explain;
pub mod find;
pub mod flash;
pub mod init;
pub mod lint;
pub mod pull;
pub mod setup;
pub mod show;
pub mod skill;
pub mod status;
pub mod upgrade_check;
