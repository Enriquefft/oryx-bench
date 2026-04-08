//! oryx-bench — a workbench for ZSA keyboard layouts.
//!
//! The binary entry point is `src/main.rs`. This library exposes the
//! modules that the binary and the `xtask` crate both depend on:
//!
//! - [`schema`] — serde types for Oryx JSON, layout.toml, features.toml,
//!   kb.toml, plus the internal canonical representation.
//! - [`pull`] — auto-pull logic and GraphQL client.
//! - [`render`] — ASCII split-grid renderer.
//! - [`lint`] — lint trait, rule registry, markdown generator.
//! - [`commands`] — one function per CLI subcommand.
//! - [`cli`] — clap definitions + command-reference markdown generator.
//! - [`skill`] — embedded skill installer.
//! - [`config`] — project root discovery + kb.toml loader.
//! - [`util`] — small utilities (fs, http, toolchain detection).
//! - [`error`] — error types.
//!
//! Read `ARCHITECTURE.md` for the canonical design.

pub mod build;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod flash;
pub mod generate;
pub mod lint;
pub mod pull;
pub mod render;
pub mod schema;
pub mod skill;
pub mod util;
