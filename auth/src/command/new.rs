use anyhow::Result;
use clap::Parser;
use jwt_simple::prelude::*;

use crate::cli::Opts;

#[derive(Debug, Clone, Parser)]
pub struct New {}

pub fn run(_global: &Opts, _opts: &New) -> Result<()> {
    // create a new key for the `HS256` JWT algorithm
    let key = HS256Key::generate();

    let mut buf = [0u8; 64];
    let key_b64 = Base64::encode_to_str(
        &mut buf,
        key.to_bytes(),
    )?;

    println!("Key:   {}", key_b64);

    // create token
    let days = 365;
    let claims = Claims::create(Duration::from_days(days));
    let token = key.authenticate(claims)?;

    println!("Token: {}", token);
    println!("This token is valid for {} days.", days);

    Ok(())
}
