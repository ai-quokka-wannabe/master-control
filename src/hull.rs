/*
    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! The convex hull of a REZ mesh: the collision proxy the exact-contacts ruling names
//! (TOPOLOGY.md § Master Control's mechanics, "Contacts are exact, because the world is planar").
//!
//! Built once at rez, in a fixed vertex order, because the hull is part of the replayed state
//! and must not depend on how a container happened to iterate: the same rows produce the same
//! hull on every run, on every machine, bit for bit. Incremental - a tetrahedron first, then
//! every point in row order, visible faces removed and the horizon re-faced - which is O(n²)
//! in the worst case and fine for the wire's cap of a thousand vertices. A flat or degenerate
//! mesh (every point coplanar) has no hull, and the body keeps the point proxy.

use std::collections::BTreeSet;

/// One face of the hull: its outward unit normal and its plane offset, `normal · p = offset`,
/// with the three hull-vertex indices that span it, anticlockwise seen from outside.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Face {
    pub normal: [f32; 3],
    pub offset: f32,
    pub vertices: [u32; 3],
}

/// The hull: its vertices (body frame, the subset of the mesh's that are extreme), its faces,
/// and its unique edges as index pairs - what the separating-axis test between two hulls will
/// enumerate.
#[derive(Clone, PartialEq, Debug)]
pub struct Hull {
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
    pub edges: Vec<[u32; 2]>,
}

const EPSILON: f32 = 1e-6;

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0].mul_add(b[0], a[1].mul_add(b[1], a[2] * b[2]))
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// A plane through three points, normal pointing away from `inside` - the hull's interior
/// reference - so every face looks outward. `None` when the three are collinear.
fn plane(points: &[[f32; 3]], a: u32, b: u32, c: u32, inside: [f32; 3]) -> Option<Face> {
    let pa = points[a as usize];
    let raw = cross(sub(points[b as usize], pa), sub(points[c as usize], pa));
    let magnitude = length(raw);
    if magnitude <= EPSILON {
        return None;
    }
    let mut normal = [raw[0] / magnitude, raw[1] / magnitude, raw[2] / magnitude];
    let mut vertices = [a, b, c];
    let mut offset = dot(normal, pa);
    if dot(normal, inside) - offset > 0.0 {
        normal = [-normal[0], -normal[1], -normal[2]];
        offset = -offset;
        vertices = [a, c, b];
    }
    Some(Face {
        normal,
        offset,
        vertices,
    })
}

fn signed_distance(face: &Face, point: [f32; 3]) -> f32 {
    dot(face.normal, point) - face.offset
}

impl Hull {
    /// The hull of `points`, or `None` when they span no volume. The tolerance scales with the
    /// mesh's own extent, so a millimetre body and a ten-metre one are judged alike.
    #[must_use]
    pub fn from_points(points: &[[f32; 3]]) -> Option<Hull> {
        if points.len() < 4 {
            return None;
        }
        let extent = points
            .iter()
            .flat_map(|point| point.iter())
            .fold(0.0f32, |largest, value| largest.max(value.abs()));
        let tolerance = EPSILON * extent.max(1.0);

        // The initial tetrahedron, chosen by rule: the first point, the farthest from it, the
        // farthest from that line, the farthest from that plane. Ties fall to the lower index.
        let first = 0u32;
        let second = farthest_by(points, |p| length(sub(p, points[0])), tolerance)?;
        let axis = sub(points[second as usize], points[0]);
        let third = farthest_by(
            points,
            |p| length(cross(sub(p, points[0]), axis)) / length(axis),
            tolerance,
        )?;
        let base = plane(points, first, second, third, points[0])?;
        let fourth = farthest_by(points, |p| signed_distance(&base, p).abs(), tolerance)?;

        let centroid = {
            let corners = [first, second, third, fourth];
            let mut sum = [0.0f32; 3];
            for corner in corners {
                let p = points[corner as usize];
                sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
            }
            [sum[0] / 4.0, sum[1] / 4.0, sum[2] / 4.0]
        };

        let mut faces: Vec<Face> = Vec::new();
        for (a, b, c) in [
            (first, second, third),
            (first, second, fourth),
            (first, third, fourth),
            (second, third, fourth),
        ] {
            faces.push(plane(points, a, b, c, centroid)?);
        }

        // Every remaining point in row order: outside some face, it replaces what it sees.
        for (index, point) in points.iter().enumerate() {
            let index = index as u32;
            if [first, second, third, fourth].contains(&index) {
                continue;
            }
            let visible: Vec<usize> = faces
                .iter()
                .enumerate()
                .filter(|(_, face)| signed_distance(face, *point) > tolerance)
                .map(|(at, _)| at)
                .collect();
            if visible.is_empty() {
                continue;
            }
            // The horizon: edges of visible faces not shared with another visible face.
            let mut horizon: Vec<[u32; 2]> = Vec::new();
            for &at in &visible {
                let [a, b, c] = faces[at].vertices;
                for edge in [[a, b], [b, c], [c, a]] {
                    let shared = visible.iter().any(|&other| {
                        other != at && {
                            let [x, y, z] = faces[other].vertices;
                            [[x, y], [y, z], [z, x]]
                                .iter()
                                .any(|e| e[0] == edge[1] && e[1] == edge[0])
                        }
                    });
                    if !shared {
                        horizon.push(edge);
                    }
                }
            }
            let mut kept: Vec<Face> = faces
                .iter()
                .enumerate()
                .filter(|(at, _)| !visible.contains(at))
                .map(|(_, face)| *face)
                .collect();
            for edge in horizon {
                if let Some(face) = plane(points, edge[0], edge[1], index, centroid) {
                    kept.push(face);
                }
            }
            faces = kept;
        }

        // The hull's own vertex list: the points its faces use, in row order, re-indexed.
        let used: BTreeSet<u32> = faces.iter().flat_map(|face| face.vertices).collect();
        let remap: Vec<u32> = used.iter().copied().collect();
        let local = |global: u32| {
            remap
                .iter()
                .position(|&g| g == global)
                .map(|at| at as u32)
                .unwrap_or(0)
        };
        let vertices: Vec<[f32; 3]> = remap.iter().map(|&g| points[g as usize]).collect();
        let faces: Vec<Face> = faces
            .into_iter()
            .map(|face| Face {
                normal: face.normal,
                offset: face.offset,
                vertices: [
                    local(face.vertices[0]),
                    local(face.vertices[1]),
                    local(face.vertices[2]),
                ],
            })
            .collect();
        let mut edges: BTreeSet<[u32; 2]> = BTreeSet::new();
        for face in &faces {
            let [a, b, c] = face.vertices;
            for [p, q] in [[a, b], [b, c], [c, a]] {
                edges.insert(if p < q { [p, q] } else { [q, p] });
            }
        }
        Some(Hull {
            vertices,
            faces,
            edges: edges.into_iter().collect(),
        })
    }

    /// The lowest body-frame y among the vertices: how far the hull reaches below the origin.
    #[must_use]
    pub fn lowest(&self) -> f32 {
        self.vertices
            .iter()
            .map(|v| v[1])
            .fold(f32::INFINITY, f32::min)
    }
}

/// The most keels a hull declares; more than this and the body is a sled with many runners,
/// and the rest are folded into the average the same way.
pub const KEELS_MAX: usize = 8;

/// How far above the hull's lowest point a vertex still counts as touching the floor: the ring
/// of tube rails around a spike, not the far ends of the tubes.
pub const KEEL_DEPTH: f32 = 0.02;

/// The most an edge may rise, per metre of run, and still lie on the floor as a runner: a tube
/// rising nine degrees behind a spike does, the tubes climbing to the crown do not.
pub const KEEL_SLOPE: f32 = 0.3;

/// The shortest edge that is a runner rather than a prism's own side.
pub const KEEL_LENGTH: f32 = 0.1;

impl Hull {
    /// The keels: the directions, in the body's horizontal plane, of the hull's edges that lie
    /// along the floor - long, low-rising edges leaving the region the body rests on. What the
    /// body slides on. A sharp point on a hard floor rubs the same in every direction; what a
    /// spiky body actually rests on is its tubes, and a tube lying along the floor is a runner:
    /// it glides along its length and ploughs across it, which is where an undulator's
    /// anisotropy comes from - and so its propulsion. Unit vectors in (x, z), sign-free (a
    /// runner is the same runner either way), at most [`KEELS_MAX`], in the hull's own order.
    /// None for a body that rests on a point: it slides the same every way.
    #[must_use]
    pub fn keels(&self) -> Vec<[f32; 2]> {
        let lowest = self.lowest();
        let mut keels = Vec::new();
        for edge in &self.edges {
            let a = self.vertices[edge[0] as usize];
            let b = self.vertices[edge[1] as usize];
            // From the region resting on the floor.
            let (near, far) = if a[1] <= b[1] { (a, b) } else { (b, a) };
            if near[1] - lowest > KEEL_DEPTH {
                continue;
            }
            let dx = far[0] - near[0];
            let dz = far[2] - near[2];
            let run = (dx * dx + dz * dz).sqrt();
            if run < KEEL_LENGTH {
                continue;
            }
            if (far[1] - near[1]) / run > KEEL_SLOPE {
                continue;
            }
            if keels.len() < KEELS_MAX {
                keels.push([dx / run, dz / run]);
            }
        }
        keels
    }
}

fn farthest_by(
    points: &[[f32; 3]],
    measure: impl Fn([f32; 3]) -> f32,
    tolerance: f32,
) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for (index, point) in points.iter().enumerate() {
        let value = measure(*point);
        if value.is_finite() && best.is_none_or(|(_, largest)| value > largest) {
            best = Some((index as u32, value));
        }
    }
    best.filter(|(_, largest)| *largest > tolerance)
        .map(|(index, _)| index)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // Expected values may use the platform's: compared within a tolerance.
mod tests {
    use super::*;

    fn cube(size: f32) -> Vec<[f32; 3]> {
        let h = size / 2.0;
        vec![
            [-h, -h, -h],
            [h, -h, -h],
            [-h, h, -h],
            [h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [-h, h, h],
            [h, h, h],
        ]
    }

    #[test]
    fn a_box_lying_on_a_face_has_keels_along_that_face_and_a_point_has_none() {
        // A box on its face: the edges of that face lie on the floor - the four sides and the
        // two diagonals the triangulation adds - in the hull's own order.
        let hull = Hull::from_points(&cube(1.0)).expect("a cube has a hull");
        let keels = hull.keels();
        assert!(keels.len() >= 4, "{keels:?}");
        for keel in &keels {
            let length = (keel[0] * keel[0] + keel[1] * keel[1]).sqrt();
            assert!((length - 1.0).abs() < 1e-5);
        }
        assert!(
            keels.iter().any(|k| k[0].abs() > 0.999),
            "a runner along x: {keels:?}"
        );
        assert!(
            keels.iter().any(|k| k[1].abs() > 0.999),
            "a runner along z: {keels:?}"
        );
        // A pyramid standing on its apex rests on a point: no runner, nothing to glide along.
        let apex_down = vec![
            [0.0, -0.5, 0.0],
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
        ];
        let hull = Hull::from_points(&apex_down).expect("a pyramid has a hull");
        assert!(hull.keels().is_empty(), "{:?}", hull.keels());
    }

    #[test]
    fn the_worm_rests_on_two_runners_thirty_degrees_either_side_of_its_axis() {
        // The real body, read from the chain golden's own rez line: the pitched icosahedron
        // standing on one spike, its tubes proud of its edges. Of the tubes meeting the spike
        // it stands on, the two that run back along the bottom face lie lowest - a V of
        // runners, thirty degrees either side of the axis, the way a sled has two.
        let log = include_str!("../tests/data/chain_life.log");
        let rez = log
            .lines()
            .find(|line| line.starts_with("rez "))
            .expect("a rez in the golden");
        let tokens: Vec<&str> = rez.split_whitespace().collect();
        let count: usize = tokens[7].parse().expect("vertex count");
        let points: Vec<[f32; 3]> = (0..count)
            .map(|index| {
                let at = 8 + index * 3;
                let bits =
                    |token: &str| f32::from_bits(u32::from_str_radix(token, 16).expect("hex"));
                [bits(tokens[at]), bits(tokens[at + 1]), bits(tokens[at + 2])]
            })
            .collect();
        let hull = Hull::from_points(&points).expect("the worm has a hull");
        let keels = hull.keels();
        assert!(!keels.is_empty(), "the worm lies on its tubes");
        // Every runner leaves the front spike backwards (+z) at about thirty degrees from the
        // axis, on one side or the other - and both sides are there.
        let mut left = false;
        let mut right = false;
        for keel in &keels {
            let backward = keel[1].abs();
            let angle = keel[0].abs().atan2(backward).to_degrees();
            assert!(
                (angle - 30.0).abs() < 6.0,
                "a runner {angle} degrees off the axis: {keels:?}"
            );
            if keel[0] * keel[1] < 0.0 {
                left = true;
            } else {
                right = true;
            }
        }
        assert!(left && right, "runners on both sides: {keels:?}");
    }

    #[test]
    fn a_cube_is_its_eight_corners_twelve_triangles_and_eighteen_edges() {
        let hull = Hull::from_points(&cube(1.0)).expect("a cube spans volume");
        assert_eq!(hull.vertices.len(), 8);
        assert_eq!(hull.faces.len(), 12);
        assert_eq!(hull.edges.len(), 18);
        assert!((hull.lowest() + 0.5).abs() < 1e-6);
        // Every face looks outward: the origin is inside every plane.
        for face in &hull.faces {
            assert!(signed_distance(face, [0.0; 3]) < 0.0, "{face:?}");
            assert!((length(face.normal) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn interior_points_vanish_and_the_order_of_rows_does_not_change_the_hull() {
        let mut points = cube(2.0);
        points.push([0.1, 0.2, 0.3]);
        points.push([0.0, 0.0, 0.0]);
        let hull = Hull::from_points(&points).expect("volume");
        assert_eq!(hull.vertices.len(), 8, "interior points are not extreme");

        let mut reversed = cube(2.0);
        reversed.reverse();
        let other = Hull::from_points(&reversed).expect("volume");
        let mut a: Vec<[u32; 3]> = hull.vertices.iter().map(|v| v.map(f32::to_bits)).collect();
        let mut b: Vec<[u32; 3]> = other.vertices.iter().map(|v| v.map(f32::to_bits)).collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "the same corners, whatever the row order");
        assert_eq!(other.faces.len(), 12);
    }

    #[test]
    fn a_flat_or_thin_mesh_has_no_hull() {
        let flat = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.5, 0.0, 0.5],
        ];
        assert!(Hull::from_points(&flat).is_none());
        let line = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
        ];
        assert!(Hull::from_points(&line).is_none());
        assert!(Hull::from_points(&cube(1.0)[..3]).is_none());
    }

    #[test]
    fn the_same_rows_give_the_same_hull_bit_for_bit() {
        let points = {
            let mut p = cube(0.4);
            p.push([0.0, 0.35, 0.0]);
            p.push([0.0, -0.1, -0.3]);
            p
        };
        let a = Hull::from_points(&points).expect("volume");
        let b = Hull::from_points(&points).expect("volume");
        assert_eq!(a, b);
        assert_eq!(a.vertices.len(), 10);
    }
}
