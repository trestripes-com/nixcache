//! Object Signing and Verification.
//!
//! Nix utilitizes Ed25519 to generate signatures on NAR hashes. Currently
//! we can either generate signatures on the fly per request, or cache them
//! in the data store.
//!
//! ## String format
//!
//! All signing-related strings in Nix follow the same format (henceforth
//! "the canonical format"):
//!
//! ```text
//! {keyName}:{base64Payload}
//! ```
//!
//! We follow the same format, so keys generated using the Nix CLI will
//! simply work.
//!
//! ## Serde
//!
//! `Serialize` and `Deserialize` are implemented to convert the structs
//! from and to the canonical format.

use anyhow::Result;
use serde::{de, ser, Deserialize, Serialize};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, DecodeError, Engine};
use displaydoc::Display;

/// An ed25519 keypair for signing.
#[derive(Debug, Clone)]
pub struct Keypair {
    /// Name of this key.
    name: String,

    /// The keypair.
    keypair: ed25519_compact::KeyPair,
}

/// An ed25519 public key for verification.
#[derive(Debug, Clone)]
pub struct PublicKey {
    /// Name of this key.
    name: String,

    /// The public key.
    public: ed25519_compact::PublicKey,
}

/// A signing error.
#[derive(Debug, Display)]
#[ignore_extra_doc_attributes]
pub enum Error {
    /// Signature error: {0}
    SignatureError(ed25519_compact::Error),

    /// The string has a wrong key name attached to it: Our name is "{our_name}" and the string has "{string_name}"
    WrongKeyName {
        our_name: String,
        string_name: String,
    },

    /// The string lacks a colon separator.
    NoColonSeparator,

    /// The name portion of the string is blank.
    BlankKeyName,

    /// The payload portion of the string is blank.
    BlankPayload,

    /// Base64 decode error: {0}
    Base64DecodeError(DecodeError),

    /// Invalid base64 payload length: Expected {expected} ({usage}), got {actual}
    InvalidPayloadLength {
        expected: usize,
        actual: usize,
        usage: &'static str,
    },

    /// Invalid signing key name "{0}".
    ///
    /// A valid name cannot be empty and must be contain colons (:).
    InvalidSigningKeyName(String),
}
impl std::error::Error for Error {}

impl Keypair {
    /// Generates a new keypair.
    pub fn generate(name: &str) -> Result<Self> {
        let keypair = ed25519_compact::KeyPair::generate();

        validate_name(name)?;

        Ok(Self {
            name: name.to_string(),
            keypair,
        })
    }

    /// Imports an existing keypair from its canonical representation.
    pub fn from_str(keypair: &str) -> Result<Self> {
        let (name, bytes) = decode_string(keypair, "keypair", ed25519_compact::KeyPair::BYTES, None)?;

        let keypair = ed25519_compact::KeyPair::from_slice(&bytes).map_err(Error::SignatureError)?;

        Ok(Self {
            name: name.to_string(),
            keypair,
        })
    }

    /// Returns the canonical representation of the keypair.
    ///
    /// This results in a 64-byte base64 payload that contains both the private
    /// key and the public key, in that order.
    ///
    /// For example, it can look like:
    ///     attic-test:msdoldbtlongtt0/xkzmcbqihd7yvy8iomajqhnkutsl3b1pyyyc0mgg2rs0ttzzuyuk9rb2zphvtpes71mlha==
    pub fn export_keypair(&self) -> String {
        format!("{}:{}", self.name, BASE64_STANDARD.encode(*self.keypair))
    }

    /// Returns the canonical representation of the public key.
    ///
    /// For example, it can look like:
    ///     attic-test:C929acssgtJoINkUtLbc81GFJPUW9maR77TxEu9ZpRw=
    pub fn export_public_key(&self) -> String {
        format!("{}:{}", self.name, BASE64_STANDARD.encode(*self.keypair.pk))
    }

    /// Returns the public key portion of the keypair.
    pub fn to_public_key(&self) -> PublicKey {
        PublicKey {
            name: self.name.clone(),
            public: self.keypair.pk,
        }
    }

    /// Signs a message, returning its canonical representation.
    pub fn sign(&self, message: &[u8]) -> String {
        let bytes = self.keypair.sk.sign(message, None);
        format!("{}:{}", self.name, BASE64_STANDARD.encode(bytes))
    }

    /// Verifies a message.
    pub fn verify(&self, message: &[u8], signature: &str) -> Result<()> {
        let (_, bytes) = decode_string(signature, "signature", ed25519_compact::Signature::BYTES, Some(&self.name))?;

        let bytes: [u8; ed25519_compact::Signature::BYTES] = bytes.try_into().unwrap();
        let signature = ed25519_compact::Signature::from_slice(&bytes).map_err(Error::SignatureError)?;

        self.keypair
            .pk
            .verify(message, &signature)
            .map_err(|e| Error::SignatureError(e).into())
    }
}

impl<'de> Deserialize<'de> for Keypair {
    /// Deserializes a potentially-invalid Nix keypair from its canonical representation.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use de::Error;
        String::deserialize(deserializer)
            .and_then(|s| Self::from_str(&s).map_err(|e| Error::custom(e.to_string())))
    }
}

impl Serialize for Keypair {
    /// Serializes a Nix keypair to its canonical representation.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.export_keypair())
    }
}

impl PublicKey {
    /// Imports an existing public key from its canonical representation.
    pub fn from_str(public_key: &str) -> Result<Self> {
        let (name, bytes) = decode_string(public_key, "public key", ed25519_compact::PublicKey::BYTES, None)?;

        let public = ed25519_compact::PublicKey::from_slice(&bytes).map_err(Error::SignatureError)?;

        Ok(Self {
            name: name.to_string(),
            public,
        })
    }

    /// Returns the Nix-compatible textual representation of the public key.
    ///
    /// For example, it can look like:
    ///     attic-test:C929acssgtJoINkUtLbc81GFJPUW9maR77TxEu9ZpRw=
    pub fn export(&self) -> String {
        format!("{}:{}", self.name, BASE64_STANDARD.encode(*self.public))
    }

    /// Verifies a message.
    pub fn verify(&self, message: &[u8], signature: &str) -> Result<()> {
        let (_, bytes) = decode_string(signature, "signature", ed25519_compact::Signature::BYTES, Some(&self.name))?;

        let bytes: [u8; ed25519_compact::Signature::BYTES] = bytes.try_into().unwrap();
        let signature = ed25519_compact::Signature::from_slice(&bytes).map_err(Error::SignatureError)?;

        self.public
            .verify(message, &signature)
            .map_err(|e| Error::SignatureError(e).into())
    }
}

/// Validates the name/label of a signing key.
///
/// A valid name cannot be empty and must not contain colons (:).
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.find(':').is_some() {
        Err(Error::InvalidSigningKeyName(name.to_string()).into())
    } else {
        Ok(())
    }
}

/// Decodes a colon-delimited string containing a key name and a base64 payload.
fn decode_string<'s>(
    s: &'s str,
    usage: &'static str,
    expected_payload_length: usize,
    expected_name: Option<&str>,
) -> Result<(&'s str, Vec<u8>)> {
    let colon = s.find(':').ok_or(Error::NoColonSeparator)?;

    let (name, colon_and_payload) = s.split_at(colon);

    validate_name(name)?;

    // don't bother decoding base64 if the name doesn't match
    if let Some(expected_name) = expected_name {
        if expected_name != name {
            return Err(Error::WrongKeyName {
                our_name: expected_name.to_string(),
                string_name: name.to_string(),
            }
            .into());
        }
    }

    let bytes = BASE64_STANDARD
        .decode(&colon_and_payload[1..])
        .map_err(Error::Base64DecodeError)?;

    if bytes.len() != expected_payload_length {
        return Err(Error::InvalidPayloadLength {
            actual: bytes.len(),
            expected: expected_payload_length,
            usage,
        }
        .into());
    }

    Ok((name, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let keypair = Keypair::generate("attic-test").expect("Could not generate key");

        let export_priv = keypair.export_keypair();
        let export_pub = keypair.export_public_key();

        eprintln!("Private key: {}", export_priv);
        eprintln!(" Public key: {}", export_pub);

        // re-import keypair
        let import = Keypair::from_str(&export_priv).expect("Could not re-import generated key");

        assert_eq!(keypair.name, import.name);
        assert_eq!(keypair.keypair, import.keypair);

        // re-import public key
        let import_pub = PublicKey::from_str(&export_pub).expect("Could not re-import public key");

        assert_eq!(keypair.name, import_pub.name);
        assert_eq!(keypair.keypair.pk, import_pub.public);

        // test the export functionality of PublicKey as well
        let export_pub2 = import_pub.export();
        let import_pub2 = PublicKey::from_str(&export_pub2).expect("Could not re-import public key");

        assert_eq!(keypair.name, import_pub2.name);
        assert_eq!(keypair.keypair.pk, import_pub2.public);
    }

    #[test]
    fn test_serde() {
        let json = "\"attic-test:x326WFy/JUl+MQnN1u9NPdWQPBbcVn2mwoIqSLS3DmQqZ8qT8rBSxxEnyhtl3jDouBqodlyfq6F+HsVhbTYPMA==\"";

        let keypair: Keypair = serde_json::from_str(json).expect("Could not deserialize keypair");

        let export = serde_json::to_string(&keypair).expect("Could not serialize keypair");

        eprintln!("Public Key: {}", keypair.export_public_key());

        assert_eq!(json, &export);
    }

    #[test]
    fn test_import_public_key() {
        let cache_nixos_org = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=";
        let import = PublicKey::from_str(cache_nixos_org).expect("Could not import public key");

        assert_eq!(cache_nixos_org, import.export());
    }

    #[test]
    fn test_signing() {
        let keypair = Keypair::generate("attic-test").expect("Could not generate key");

        let public = keypair.to_public_key();

        let message = b"hello world";

        let signature = keypair.sign(message);

        keypair.verify(message, &signature).unwrap();
        public.verify(message, &signature).unwrap();

        keypair.verify(message, "attic-test:lo9EfNIL4eGRuNh7DTbAAffWPpI2SlYC/8uP7JnhgmfRIUNGhSbFe8qEaKN0mFS02TuhPpXFPNtRkFcCp0hGAQ==").unwrap_err();
    }
}
