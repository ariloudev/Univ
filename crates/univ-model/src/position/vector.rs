//! Cartesian vector arithmetic, and the angle between two directions.

use std::array::from_fn;

use super::Vec3;

/// The dot product.
#[must_use]
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `f` applied to the matching components of `a` and `b`.
#[must_use]
pub fn zip(a: Vec3, b: Vec3, f: impl Fn(f64, f64) -> f64) -> Vec3 {
    from_fn(|i| f(a[i], b[i]))
}

/// `v` times `s`.
#[must_use]
pub fn scaled(v: Vec3, s: f64) -> Vec3 {
    v.map(|c| c * s)
}

/// `a + b`.
#[must_use]
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    zip(a, b, |x, y| x + y)
}

/// `a − b`.
#[must_use]
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    zip(a, b, |x, y| x - y)
}

/// The length of `v`.
#[must_use]
pub fn norm(v: Vec3) -> f64 {
    dot(v, v).sqrt()
}

/// The cross product.
#[must_use]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The angle between the directions of `a` and `b`, degrees, as `atan2(|a × b|, a · b)`: exact
/// at small and large angles alike, where `acos` of the dot product loses its digits near 0 and
/// 180°, and the same for vectors of any length.
#[must_use]
pub fn separation_degrees(a: Vec3, b: Vec3) -> f64 {
    let c = cross(a, b);
    dot(c, c).sqrt().atan2(dot(a, b)).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::unit_vector;

    #[test]
    fn a_separation_keeps_its_digits_at_a_milliarcsecond() {
        let (a, b) = (
            unit_vector(10.0, 20.0),
            unit_vector(10.0, 20.0 + 1e-3 / 3_600.0),
        );
        let mas = separation_degrees(a, b) * 3_600_000.0;
        assert!((mas - 1.0).abs() < 1e-6, "{mas}");
        assert!((separation_degrees(a, scaled(a, -2.0)) - 180.0).abs() < 1e-12);
    }
}
