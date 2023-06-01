use anyhow::Result;
use clap::{Parser, Subcommand};
use enum_as_inner::EnumAsInner;

use crate::command::new::{self, New};

/// Nixcache auth token util.
#[derive(Debug, Parser)]
#[clap(version)]
#[clap(propagate_version = true)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, EnumAsInner)]
pub enum Command {
    New(New),
}

pub fn run() -> Result<()> {
    let opts = Opts::parse();

    match opts.command {
        Command::New(ref sub) => new::run(&opts, &sub),
    }
}
