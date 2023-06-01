use anyhow::Result;
use jwt_simple::prelude::Base64;
use jwt_simple::reexports::ct_codecs::Decoder;

pub use jwt_simple::prelude::{HS256Key, NoCustomClaims, MACLike};
pub use jwt_simple::Error as JWTError;

pub fn decode_token_hs256_secret_base64(s: &str) -> Result<HS256Key> {
    let mut buf = [0u8; 64];
    let secret = Base64::decode(&mut buf, s, None)?;
    Ok(HS256Key::from_bytes(&secret))
}
