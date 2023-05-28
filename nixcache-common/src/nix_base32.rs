//! # Encodes `[u8]` as base32 like nix
//!
//! This crate encodes a `[u8]` byte slice in a nix-compatible way.
//! SHA256 hash codes in [nix](https://nixos.org/nix/) are usually encoded in base32 with
//! an unusual set of characters (without E O U T).

// omitted: E O U T
const BASE32_CHARS: &[u8] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Converts the given byte slice to a nix-compatible base32 encoded String.
pub fn to_nix_base32(bytes: &[u8]) -> String {
    let len = (bytes.len() * 8 - 1) / 5 + 1;

    (0..len)
        .rev()
        .map(|n| {
            let b: usize = (n as usize) * 5;
            let i: usize = b / 8;
            let j: usize = b % 8;
            // bits from the lower byte
            let v1 = bytes[i].checked_shr(j as u32).unwrap_or(0);
            // bits from the upper byte
            let v2 = if i >= bytes.len() - 1 {
                0
            } else {
                bytes[i + 1].checked_shl(8 - j as u32).unwrap_or(0)
            };
            let v: usize = (v1 | v2) as usize;
            char::from(BASE32_CHARS[v % BASE32_CHARS.len()])
        })
        .collect()
}

/// Converts the given nix-compatible base32 encoded String to a byte vector.
pub fn from_nix_base32(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    let hash_size = s.len() * 5 / 8;
    let mut hash: Vec<u8> = vec![0; hash_size];

    for n in 0usize..s.len() {
        let c = s[s.len() - n - 1];
        let digit = BASE32_CHARS.iter().position(|b| *b == c)? as u8;
        let b = n * 5;
        let i = b / 8;
        let j = b % 8;
        hash[i] |= digit.checked_shl(j as u32).unwrap_or(0);

        let v2 = digit.checked_shr(8 - j as u32).unwrap_or(0);
        if i < hash_size - 1 {
            hash[i + 1] |= v2;
        } else if v2 != 0 {
            return None;
        }
    }

    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base32() {
        let cases = [
            // hex, base32
            ("ab335240fd942ab8191c5e628cd4ff3903c577bda961fb75df08e0303a00527b", "0ysj00x31q08vxsznqd9pmvwa0rrzza8qqjy3hcvhallzm054cxb"),

            // NAR hashes of /nix/store/0q43idch209zsdngl8yl79x0y79aajib-nix-2.8.1
            ("47b2d8f260c2d48116044bc43fe3de0f", "0gvvikzi2b0hb83m62c3rdicj7"),
            ("1f74d74729abdc08f4f84e8f7f8c808c8ed92ee5", "wlpdk3lch267z3sfz3s0ip5b553xfx0z"),
            ("a315ab26a0c4829321730c44a26f4497f7da0631402669caa4e24bdcd9db7c87", "11vwvgcxqjz2lk56j9j0643dmxwp8ips4i0cfchr70n4l0kan5d3"),
            ("296a445bfa5e1990af299ec74582468ab7a77e495861691ff79ab21234e514b64fc72b294d7305ecdd0febaa13b1bc1a3f359a711bb93dfb2b82804c64354dab", "2mlsdb49j084azv7nwinwcs6lzimg5i2fmfn3yxxh2p6k995g3lzdhlwls15clsywgnjqaq95zagdwa8s14biwy56pr06ayz9dl8si9"),

            // https://cache.nixos.org/nar/000y5y39fnxp2ijj8cmdgvmia6wwcrws1q6fbcr1fkf5rs2dm8lr.nar.xz
            ("99a2da84cec54d17325bcee0a079669c1b15eb7ead32246514b75b97862f1e00", "000y5y39fnxp2ijj8cmdgvmia6wwcrws1q6fbcr1fkf5rs2dm8lr"),
        ];

        for (hex, base32) in cases {
            assert_eq!(
                to_nix_base32(&hex::decode(hex).unwrap()),
                base32,
            );
            assert_eq!(
                from_nix_base32(base32).unwrap(),
                hex::decode(hex).unwrap()
            );
        }
    }
}
