use num_bigint::BigUint;
use num_traits::{Num, Zero};

const MODULUS: &str = "94dd2a8675fb779e6b9f7103698634cd400f27a154afa67af6166a43fc26417222a79506d34cacc7641946abda1785b7acf9910ad6a0978c91ec84d40b71d2891379af19ffb333e7517e390bd26ac312fe940c340466b4a5d4af1d65c3b5944078f96a1a51a5a53e4bc302818b7c9f63c4a1b07bd7d874cef1c3d4b2f5eb7871";
const EXPONENT: &str = "10001";

pub fn encrypt_password(password: &str) -> Result<String, Box<dyn std::error::Error>> {
    let modulus = BigUint::from_str_radix(MODULUS, 16)?;
    let exponent = BigUint::from_str_radix(EXPONENT, 16)?;
    let chunk_size = 2 * (modulus.bits() as usize).div_ceil(8).saturating_sub(1);
    let reversed: String = password.chars().rev().collect();
    let mut bytes = reversed.into_bytes();
    while !bytes.len().is_multiple_of(chunk_size) {
        bytes.push(0);
    }

    let mut result = Vec::new();
    for chunk in bytes.chunks(chunk_size) {
        let mut block = BigUint::zero();
        for (index, pair) in chunk.chunks(2).enumerate() {
            let low = pair.first().copied().unwrap_or(0) as u64;
            let high = pair.get(1).copied().unwrap_or(0) as u64;
            block += BigUint::from(low | (high << 8)) << (index * 16);
        }
        let encrypted = block.modpow(&exponent, &modulus);
        result.push(format!("{encrypted:x}"));
    }
    Ok(result.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_portal_javascript_result() {
        let actual = encrypt_password("testPassword123>aabbccddeeff").unwrap();
        assert_eq!(
            actual,
            "71292489a26b05325b8498d89cc4381a02712b3c6d7c5a57f032e9a65578e66b1131b508f163cfb0dcc920439cb229302ddda8034aeca055a89f9fdb026f5f3ba64d4885f4c332621949cdfe8c2cfae2a736b55585823e2e1082f236c9f0def4bd3987b1e90a54cc710832ad61b6951311e223638a3cc5fd8f11c78b50c07318"
        );
    }
}
