//! The `HEALPix` nested scheme (Górski et al. 2005, `ApJ` 622, 759) as far as sky regions need
//! it: the cell of an order that holds a direction, each cell's centre, and a bound on how far
//! a cell reaches from its centre. It follows `healpix_cxx`'s `loc2pix` and `pix2loc`, and a
//! test of univ-ingest holds it to `cdshealpix`.

use std::cmp::Ordering;
use std::f64::consts::{FRAC_PI_2, PI};

use crate::position::Vec3;

/// The highest order a sky region uses: cells of under a milliarcsecond.
pub const MAX_ORDER: u8 = 29;

/// For each of the 12 base cells, its ring and its offset along the ring, in `healpix_cxx`'s
/// tables `jrll` and `jpll`.
const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

/// The cell of order `order` (at most [`MAX_ORDER`]) that holds the direction of `v`, which
/// need not be a unit vector but must not be zero.
#[must_use]
pub fn hash(order: u8, v: Vec3) -> u64 {
    let order = order.min(MAX_ORDER);
    let nside = 1_i64 << order;
    let n = real(nside);
    let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let z = v[2] / norm;
    let za = z.abs();
    let tt = (v[1].atan2(v[0]) / FRAC_PI_2).rem_euclid(4.0);
    let (face, ix, iy) = if za <= 2.0 / 3.0 {
        let temp1 = n * (0.5 + tt);
        let temp2 = n * z * 0.75;
        let jp = index(temp1 - temp2);
        let jm = index(temp1 + temp2);
        let (ifp, ifm) = (jp >> order, jm >> order);
        let face = match ifp.cmp(&ifm) {
            Ordering::Equal => ifp | 4,
            Ordering::Less => ifp,
            Ordering::Greater => ifm + 8,
        };
        (face, jm & (nside - 1), nside - (jp & (nside - 1)) - 1)
    } else {
        let ntt = index(tt).min(3);
        let tp = tt - real(ntt);
        // Near a pole 1 − |z| loses digits: the sine of the colatitude keeps them.
        let tmp = if za > 0.99 {
            let sth = v[0].hypot(v[1]) / norm;
            n * sth / ((1.0 + za) / 3.0).sqrt()
        } else {
            n * (3.0 * (1.0 - za)).sqrt()
        };
        let jp = index(tp * tmp).min(nside - 1);
        let jm = index((1.0 - tp) * tmp).min(nside - 1);
        if z >= 0.0 {
            (ntt, nside - jm - 1, nside - jp - 1)
        } else {
            (ntt + 8, jp, jm)
        }
    };
    #[expect(
        clippy::cast_sign_loss,
        reason = "a face and cell indices are not negative"
    )]
    let hash = ((face as u64) << (2 * order)) + spread(ix as u64) + (spread(iy as u64) << 1);
    hash
}

/// The unit vector toward the centre of cell `hash` of order `order`.
#[must_use]
pub fn centre(order: u8, hash: u64) -> Vec3 {
    let order = order.min(MAX_ORDER);
    let nside = 1_i64 << order;
    let n = real(nside);
    let face = usize::from(u8::try_from((hash >> (2 * order)) % 12).unwrap_or(0));
    let pixel = hash & ((1_u64 << (2 * order)) - 1);
    #[expect(clippy::cast_possible_wrap, reason = "cell indices are under 2^29")]
    let (ix, iy) = (compress(pixel) as i64, compress(pixel >> 1) as i64);
    let jr = (JRLL[face] << order) - ix - iy - 1;
    let (nr, z, kshift) = if jr < nside {
        let nr = jr;
        (nr, 1.0 - real(nr * nr) / (3.0 * n * n), 0)
    } else if jr > 3 * nside {
        let nr = 4 * nside - jr;
        (nr, real(nr * nr) / (3.0 * n * n) - 1.0, 0)
    } else {
        (
            nside,
            real(2 * nside - jr) * 2.0 / (3.0 * n),
            (jr - nside) & 1,
        )
    };
    let mut jp = (JPLL[face] * nr + ix - iy + 1 + kshift) / 2;
    if jp > 4 * nside {
        jp -= 4 * nside;
    }
    if jp < 1 {
        jp += 4 * nside;
    }
    let phi = (real(jp) - real(kshift + 1) * 0.5) * (FRAC_PI_2 / real(nr));
    let sth = (1.0 - z * z).max(0.0).sqrt();
    [sth * phi.cos(), sth * phi.sin(), z]
}

/// An upper bound, in radians, on the angle from a cell's centre to any point of the cell at
/// order `order`: a cell's side, `√(π/3) / 2^order`, with a margin that a test of univ-ingest
/// checks against `cdshealpix`'s vertices.
#[must_use]
pub fn radius_bound(order: u8) -> f64 {
    1.25 * (PI / 3.0).sqrt() / f64::from(1_u32 << order.min(31).min(30))
}

/// An index of a cell or ring, under 2^31 in magnitude, as a float: exact.
#[expect(
    clippy::cast_precision_loss,
    reason = "indices are under 2^31, which f64 holds exactly"
)]
fn real(i: i64) -> f64 {
    i as f64
}

/// The integer part of a non-negative coordinate scaled to cells, under 2^31.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a direction's scaled coordinate is under 4 · 2^29; the cast takes its integer part"
)]
fn index(x: f64) -> i64 {
    x as i64
}

/// The bits of `v` spread to the even bits of the result.
fn spread(v: u64) -> u64 {
    let mut v = v & 0xffff_ffff;
    v = (v | (v << 16)) & 0x0000_ffff_0000_ffff;
    v = (v | (v << 8)) & 0x00ff_00ff_00ff_00ff;
    v = (v | (v << 4)) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | (v << 2)) & 0x3333_3333_3333_3333;
    (v | (v << 1)) & 0x5555_5555_5555_5555
}

/// The even bits of `v`, packed.
fn compress(v: u64) -> u64 {
    let mut v = v & 0x5555_5555_5555_5555;
    v = (v | (v >> 1)) & 0x3333_3333_3333_3333;
    v = (v | (v >> 2)) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | (v >> 4)) & 0x00ff_00ff_00ff_00ff;
    v = (v | (v >> 8)) & 0x0000_ffff_0000_ffff;
    (v | (v >> 16)) & 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_poles_and_the_equator_fall_in_the_cells_of_their_base_faces() {
        assert!(
            hash(0, [0.0, 0.0, 1.0]) < 4,
            "the north pole is in faces 0 to 3"
        );
        assert!(
            hash(0, [0.0, 0.0, -1.0]) >= 8,
            "the south pole in faces 8 to 11"
        );
        assert_eq!(
            hash(0, [1.0, 0.0, 0.0]),
            4,
            "(0, 0) is the centre of face 4"
        );
    }

    #[test]
    fn each_cells_centre_lies_in_that_cell() {
        for order in [0_u8, 1, 3, 8, 12] {
            let cells = 12_u64 << (2 * order);
            let step = usize::try_from((cells / 997).max(1)).unwrap();
            for hash_ in (0..cells).step_by(step) {
                assert_eq!(hash(order, centre(order, hash_)), hash_, "order {order}");
            }
        }
    }

    #[test]
    fn bits_spread_and_compress_back() {
        for v in [0_u64, 1, 2, 3, 0xabcd, (1 << 29) - 1] {
            assert_eq!(compress(spread(v)), v);
        }
    }
}
