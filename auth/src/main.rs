mod cli;
mod command;

use anyhow::Result;

fn main() -> Result<()> {
    cli::run()
}
