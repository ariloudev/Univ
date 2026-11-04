//! The tile nodes that cover a volume (query-a-bounded-volume): for each scale domain, the
//! nodes of its octree at the deepest level, no deeper than asked, at which at most
//! [`MAX_TILE_NODES`] nodes meet the volume. A covering at a level is the level's nodes that
//! meet the volume and the leaves above it that do, so that where the octree stops short of
//! the level its deepest nodes stand in. Each address is one the domain's tile route serves.

use univ_model::api::query::{MAX_TILE_NODES, TileCovering};
use univ_model::units::{Unit, scale_domain_unit};
use univ_model::volume::Region;

use super::Query;
use crate::backend::Backend;
use crate::bulk::{NodeCube, domains_of, tile_nodes};
use crate::problem::Problem;

/// The covering of each scale domain the query's filters leave.
pub(super) async fn coverings<B: Backend>(
    backend: &B,
    query: &Query,
    max_level: Option<u8>,
) -> Result<Vec<TileCovering>, Problem> {
    let mut coverings = Vec::new();
    for domain in domains_of(backend, query.release).await?.iter() {
        let wanted = query
            .filters
            .scale_domains
            .as_ref()
            .is_none_or(|names| names.contains(&domain.name));
        let Ok(factor) = Unit::Megaparsec.factor_to(scale_domain_unit(&domain.name)) else {
            continue;
        };
        if !wanted {
            continue;
        }
        let nodes = tile_nodes(backend, query.release, &domain.name, &domain.manifest_key).await?;
        let (level, nodes) = covering(
            &nodes.cubes,
            &query.region.clone().scaled(factor),
            max_level,
        );
        coverings.push(TileCovering {
            scale_domain: domain.name.clone(),
            level,
            nodes,
        });
    }
    Ok(coverings)
}

/// The level chosen among `cubes` for `region`, in their unit, and its nodes by address.
pub(crate) fn covering(
    cubes: &[NodeCube],
    region: &Region,
    max_level: Option<u8>,
) -> (u8, Vec<String>) {
    let meeting: Vec<&NodeCube> = cubes
        .iter()
        .filter(|c| region.meets_box(&c.bounds))
        .collect();
    let deepest = cubes.iter().map(|c| c.level).max().unwrap_or(0);
    let top = max_level.map_or(deepest, |m| m.min(deepest));
    for level in (0..=top).rev() {
        let mut nodes: Vec<String> = meeting
            .iter()
            .filter(|c| c.level == level || (c.leaf && c.level < level))
            .map(|c| c.address.clone())
            .collect();
        if nodes.len() <= MAX_TILE_NODES || level == 0 {
            nodes.sort();
            return (level, nodes);
        }
    }
    (0, Vec::new())
}

#[cfg(test)]
mod tests {
    use univ_model::geometry::Aabb;

    use super::*;

    /// A full octree of `depth` levels below a root from -64 to 64 on each axis.
    fn full(depth: u8) -> Vec<NodeCube> {
        let mut cubes = Vec::new();
        let mut level = vec![(
            "r".to_owned(),
            Aabb {
                min: [-64.0; 3],
                max: [64.0; 3],
            },
        )];
        for l in 0..=depth {
            let mut below = Vec::new();
            for (address, bounds) in level {
                if l < depth {
                    for octant in 0..8_u8 {
                        let half: [f64; 3] =
                            std::array::from_fn(|i| (bounds.max[i] - bounds.min[i]) / 2.0);
                        let min: [f64; 3] = std::array::from_fn(|i| {
                            bounds.min[i] + if octant >> i & 1 == 1 { half[i] } else { 0.0 }
                        });
                        let max = std::array::from_fn(|i| min[i] + half[i]);
                        below.push((format!("{address}{octant}"), Aabb { min, max }));
                    }
                }
                cubes.push(NodeCube {
                    address,
                    level: l,
                    bounds,
                    leaf: l == depth,
                });
            }
            level = below;
        }
        cubes
    }

    /// The nodes of `level` a sphere meets, counted the plain way: the distance from its
    /// centre to each cube.
    fn brute(cubes: &[NodeCube], centre: [f64; 3], radius: f64, level: u8) -> usize {
        cubes
            .iter()
            .filter(|c| c.level == level)
            .filter(|c| {
                let d: f64 = (0..3)
                    .map(|i| {
                        (c.bounds.min[i] - centre[i])
                            .max(centre[i] - c.bounds.max[i])
                            .max(0.0)
                            .powi(2)
                    })
                    .sum();
                d.sqrt() <= radius
            })
            .count()
    }

    #[test]
    fn the_level_is_the_deepest_at_which_at_most_4096_nodes_meet_the_volume() {
        let cubes = full(5);
        for (centre, radius) in [
            ([0.0; 3], 60.0),
            ([10.0, -20.0, 5.0], 25.0),
            ([40.0, 40.0, 40.0], 3.0),
        ] {
            let region = Region::Sphere { centre, radius };
            let (level, nodes) = covering(&cubes, &region, None);
            let counts: Vec<usize> = (0..=5).map(|l| brute(&cubes, centre, radius, l)).collect();
            let expected = (0..=5_u8)
                .rev()
                .find(|&l| counts[usize::from(l)] <= MAX_TILE_NODES)
                .unwrap();
            assert_eq!(level, expected, "{centre:?} {radius}: {counts:?}");
            assert_eq!(nodes.len(), counts[usize::from(level)]);
        }
        let region = Region::Sphere {
            centre: [0.0; 3],
            radius: 60.0,
        };
        assert_eq!(
            covering(&cubes, &region, None).0,
            4,
            "level 5 has more than 4,096 nodes"
        );
        assert_eq!(
            covering(&cubes, &region, Some(2)).0,
            2,
            "no deeper than asked"
        );
        assert_eq!(covering(&cubes, &region, Some(2)).1.len(), 64);
    }

    #[test]
    fn where_the_octree_stops_short_its_leaves_stand_in() {
        let mut cubes = full(1);
        for cube in &mut cubes {
            cube.leaf = cube.level == 1 && cube.address != "r0";
        }
        let r0 = cubes.iter().find(|c| c.address == "r0").unwrap().bounds;
        for octant in 0..8 {
            let half = 32.0;
            let min: [f64; 3] =
                std::array::from_fn(|i| r0.min[i] + if octant >> i & 1 == 1 { half } else { 0.0 });
            let max = std::array::from_fn(|i| min[i] + half);
            cubes.push(NodeCube {
                address: format!("r0{octant}"),
                level: 2,
                bounds: Aabb { min, max },
                leaf: true,
            });
        }
        let everywhere = Region::Sphere {
            centre: [0.0; 3],
            radius: 200.0,
        };
        let (level, nodes) = covering(&cubes, &everywhere, None);
        assert_eq!(level, 2);
        let expected: Vec<String> = [
            "r00", "r01", "r02", "r03", "r04", "r05", "r06", "r07", "r1", "r2", "r3", "r4", "r5",
            "r6", "r7",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(nodes, expected);
    }
}
