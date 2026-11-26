//! Marking the model points that real objects cover (build-the-galaxy-model): in each cube of
//! [`COVER_CUBE_PC`] about the barycentre, anchored at the origin, the first `n` of a
//! population's points in the order they were sampled, `n` the real objects of that
//! population's counterpart in the cube.

use std::collections::HashMap;

use crate::position::Vec3;

/// The edge of a cube the model's points are covered in, parsecs.
pub const COVER_CUBE_PC: f64 = 250.0;

/// The real objects a population of the model has for counterparts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Counterpart {
    /// The objects of the stars domain: the counterparts of the discs, the bar and bulge and
    /// the halo.
    Stars,
    /// The luminous hot stars, open clusters and star-forming regions of the Galaxy tracers
    /// domain: the counterparts of the arms' young stars and star-forming knots.
    YoungTracers,
}

/// The cube of `p`, ICRS parsecs.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "a cube index of a position within the Galaxy"
)]
pub fn cube_of(p: Vec3) -> [i64; 3] {
    p.map(|c| (c / COVER_CUBE_PC).floor() as i64)
}

/// The real objects of each counterpart in each cube.
#[derive(Debug, Default)]
pub struct Counts(HashMap<([i64; 3], Counterpart), u32>);

impl Counts {
    /// Counts a real object of `counterpart` at `p`, ICRS parsecs.
    pub fn add(&mut self, p: Vec3, counterpart: Counterpart) {
        *self.0.entry((cube_of(p), counterpart)).or_default() += 1;
    }

    /// The real objects of `counterpart` in `cube`.
    #[must_use]
    pub fn get(&self, cube: [i64; 3], counterpart: Counterpart) -> u32 {
        self.0
            .get(&(cube, counterpart))
            .copied()
            .unwrap_or_default()
    }
}

/// Whether each of `points` is covered, in their order: the points as (position, population
/// code), `counterpart` each population's, `counts` the real objects.
#[must_use]
pub fn covered(
    points: impl IntoIterator<Item = (Vec3, u8)>,
    counterpart: impl Fn(u8) -> Counterpart,
    counts: &Counts,
) -> Vec<bool> {
    let mut taken: HashMap<([i64; 3], u8), u32> = HashMap::new();
    points
        .into_iter()
        .map(|(p, population)| {
            let cube = cube_of(p);
            let n = counts.get(cube, counterpart(population));
            let so_far = taken.entry((cube, population)).or_default();
            let covered = *so_far < n;
            if covered {
                *so_far += 1;
            }
            covered
        })
        .collect()
}
