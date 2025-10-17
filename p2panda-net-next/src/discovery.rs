// SPDX-License-Identifier: MIT OR Apache-2.0

use curve25519_dalek::{RistrettoPoint, Scalar};
use sha2::Sha512;

pub fn to_ristretto(data: &[Vec<u8>]) -> Vec<RistrettoPoint> {
    data.iter()
        .map(|item| RistrettoPoint::hash_from_bytes::<Sha512>(item))
        .collect()
}

pub fn scalar_mult(scalar: Scalar, data: &[RistrettoPoint]) -> Vec<RistrettoPoint> {
    data.iter().map(|item| item * scalar).collect()
}

#[cfg(test)]
mod tests {
    use curve25519_dalek::Scalar;
    use rand_core::OsRng;

    use super::{scalar_mult, to_ristretto};

    #[test]
    fn test_scalar_mult() {
        let mut rng = OsRng;
        let a_scalar = Scalar::random(&mut rng);
        let b_scalar = Scalar::random(&mut rng);

        let test_data = [vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        let a_mixed = scalar_mult(a_scalar, &to_ristretto(&test_data));
        let b_mixed = scalar_mult(b_scalar, &to_ristretto(&test_data));

        let b_of_a_mixed = scalar_mult(b_scalar, &a_mixed);
        let a_of_b_mixed = scalar_mult(a_scalar, &b_mixed);

        assert_eq!(b_of_a_mixed, a_of_b_mixed);
    }
}
