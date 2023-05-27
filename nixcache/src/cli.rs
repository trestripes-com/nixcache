use anyhow::Result;
use clap::{Parser, Subcommand};
use enum_as_inner::EnumAsInner;

use crate::command::push::{self, Push};

/// Nixcache.
#[derive(Debug, Parser)]
#[clap(version)]
#[clap(propagate_version = true)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, EnumAsInner)]
pub enum Command {
    Push(Push),
}

pub async fn run() -> Result<()> {
    let opts = Opts::parse();

    match opts.command {
        Command::Push(_) => push::run(opts).await,
    }
}
