use anyhow::Result;
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use enum_as_inner::EnumAsInner;

use crate::command::init::{self, Init};
use crate::command::push::{self, Push};

/// Nixcache.
#[derive(Debug, Parser)]
#[clap(version)]
#[clap(propagate_version = true)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: Command,
    /// Path to the 'config.toml'.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand, EnumAsInner)]
pub enum Command {
    Init(Init),
    Push(Push),
}

pub async fn run() -> Result<()> {
    let opts = Opts::parse();

    match opts.command {
        Command::Init(_) => init::run(opts).await,
        Command::Push(_) => push::run(opts).await,
    }
}
