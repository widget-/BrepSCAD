//! Classic (0.3.2) face division.
//!
//! Ported verbatim from the published 0.3.2 crate's `transversal::divide_face`:
//! projects each loop's edges into the face parameter domain, splits the face
//! into positively oriented pre-faces with their contained holes, and tags each
//! with its `and`/`or`/`unknown` status.

#![allow(clippy::many_single_char_names)]

use super::faces_classification::FacesClassification;
use super::loops_store::*;
use monstertruck_meshing::prelude::*;
use monstertruck_topology::*;
use rustc_hash::FxHashMap as HashMap;
use std::ops::Deref;

/// Keep a parameter-space polygon contiguous across a periodic axis, then put
/// it in the canonical image whose minimum lies inside the first period.
///
/// A `RevolutionSurface`'s turn angle is periodic, and `search_parameter`
/// returns it in `[0, 2π)`. A vertex on the seam can come back as `2π` instead
/// of `0` (the sign of `proj_angle`'s cross product decides), which turns a
/// rectangle into a self-overlapping polygon with the wrong signed area - a
/// piece of the face then reads as its complement and is dropped. Unwrapping
/// against the previous point fixes the seam; the final shift makes every loop
/// of the face agree on one periodic image.
fn unwrap_periodic(points: &mut [Point2], period_u: Option<f64>, period_v: Option<f64>) {
    for (component, period) in [(0usize, period_u), (1usize, period_v)] {
        let Some(period) = period.filter(|period| *period > 0.0) else {
            continue;
        };
        for index in 1..points.len() {
            let previous = points[index - 1][component];
            let value = points[index][component];
            points[index][component] = value + ((previous - value) / period).round() * period;
        }
        let minimum = points
            .iter()
            .map(|point| point[component])
            .fold(f64::INFINITY, f64::min);
        if minimum.is_finite() {
            let shift = (minimum / period).floor() * period;
            for point in points.iter_mut() {
                point[component] -= shift;
            }
        }
    }
}

fn create_parameter_boundary<P, C, S>(
    face: &Face<P, C, S>,
    wire: &Wire<P, C>,
    polys: &mut HashMap<EdgeId<C>, PolylineCurve<P>>,
    tol: f64,
) -> Option<PolylineCurve<Point2>>
where
    P: Copy,
    C: BoundedCurve<Point = P> + ParameterDivision1D<Point = P>,
    S: Clone + ParametricSurface3D + SearchParameter<SurfaceParameter, Point = P>,
{
    let surface = face.surface();
    let pt = wire.front_vertex().unwrap().point();
    let p: Point2 = surface.search_parameter(pt, None, 100)?.into();
    let mut vec = wire.edge_iter().try_fold(vec![p], |mut vec, edge| {
        let poly = polys.entry(edge.id()).or_insert_with(|| {
            let curve = edge.curve();
            let div = curve.parameter_division(curve.range_tuple(), tol).1;
            PolylineCurve(div)
        });
        let mut p = *vec.last().unwrap();
        let closure = |q: &P| -> Option<Point2> {
            p = surface.search_parameter(*q, Some(p.into()), 100)?.into();
            Some(p)
        };
        let add: Option<Vec<Point2>> = match edge.orientation() {
            true => poly.iter().skip(1).map(closure).collect(),
            false => poly.iter().rev().skip(1).map(closure).collect(),
        };
        vec.append(&mut add?);
        Some(vec)
    })?;
    unwrap_periodic(&mut vec, surface.period_u(), surface.period_v());
    Some(PolylineCurve(vec))
}

fn beside(poly: &PolylineCurve<Point2>) -> Point2 {
    poly.windows(2)
        .map(|pair| (pair[0], pair[1] - pair[0]))
        .max_by(|a, b| a.1.magnitude2().total_cmp(&b.1.magnitude2()))
        .map_or_else(
            || poly.front(),
            |(start, along)| start + along * 0.5 + Vector2::new(-along.y, along.x) * 1.0e-3,
        )
}

#[derive(Clone, Debug)]
struct WireChunk<'a, C> {
    poly: PolylineCurve<Point2>,
    wire: &'a BoundaryWire<Point3, C>,
}

type FaceWithShapesOpStatus<C, S> = (Face<Point3, C, S>, ShapesOpStatus);
fn divide_one_face<C, S>(
    face: &Face<Point3, C, S>,
    loops: &Loops<Point3, C>,
    tol: f64,
) -> Option<Vec<FaceWithShapesOpStatus<C, S>>>
where
    C: BoundedCurve<Point = Point3> + ParameterDivision1D<Point = Point3>,
    S: Clone + SearchParameter<SurfaceParameter, Point = Point3> + ParametricSurface3D,
{
    let (mut pre_faces, mut negative_wires) = (Vec::new(), Vec::new());
    let mut map = HashMap::default();
    let surface = face.surface();
    if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for vertex in face.vertex_iter() {
            let point = vertex.point();
            for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
        }
        eprintln!(
            "TRACE divide: face {min:.3?}..{max:.3?} loops={}",
            loops.len()
        );
    }
    let scale = |at: Point2| {
        surface
            .uder(at.x, at.y)
            .cross(surface.vder(at.x, at.y))
            .magnitude()
    };
    let negligible = |area: f64, at: Point2| (area * scale(at)).abs() < tol * tol;
    loops.iter().try_for_each(|wire| {
        let poly = create_parameter_boundary(face, wire, &mut map, tol)?;
        let area = poly.area();
        if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
            eprintln!(
                "TRACE divide: loop area={area:.6} status={:?} boundary={} face_orientation={}",
                wire.status(),
                wire.is_boundary(),
                face.orientation()
            );
            if area < 0.0 {
                let points = poly
                    .iter()
                    .map(|p| format!("({:.3}, {:.3})", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("TRACE divide: negative loop points {points}");
            }
        }
        if negligible(area, poly.front()) {
            if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                eprintln!("TRACE divide: skipped a negligible loop, area={area:.6}");
            }
            return Some(());
        }
        match area > 0.0 {
            true => pre_faces.push(vec![WireChunk { poly, wire }]),
            false => negative_wires.push(WireChunk { poly, wire }),
        }
        Some(())
    })?;
    negative_wires.into_iter().try_for_each(|chunk| {
        let pt = beside(&chunk.poly);
        let chunk_area = chunk.poly.area();
        let containing: Vec<usize> = (0..pre_faces.len())
            .filter(|&i| !pre_faces[i].is_empty() && pre_faces[i][0].poly.include(pt))
            .collect();
        let is_inverse = |i: usize| negligible(pre_faces[i][0].poly.area() + chunk_area, pt);
        let parent = containing
            .iter()
            .copied()
            .filter(|&i| !is_inverse(i))
            .min_by(|&a, &b| {
                pre_faces[a][0]
                    .poly
                    .area()
                    .total_cmp(&pre_faces[b][0].poly.area())
            });
        match (parent, containing.first()) {
            (Some(i), _) => pre_faces[i].push(chunk),
            // If the sum of areas is zero, the face is canceled.
            // This happens when an intersection loop exactly matches the face boundary.
            (None, Some(&i)) => {
                if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                    eprintln!(
                        "TRACE divide: canceled pre-face {i}, chunk_area={chunk_area:.6}, loop_area={:.6}",
                        pre_faces[i][0].poly.area()
                    );
                }
                pre_faces[i].clear();
            }
            (None, None) => {
                if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                    eprintln!("TRACE divide: dropped a negative loop with no parent");
                }
            }
        }
        Some(())
    })?;
    // PATCH(BrepSCAD): only a face whose whole boundary was overwritten by an
    // intersection curve needs the expensive outline projection. Detect that
    // case before the per-pre-face status pass.
    let boundary_only_area = if pre_faces.len() == 1
        && pre_faces[0].len() == 1
        && pre_faces[0][0].wire.status() != ShapesOpStatus::Unknown
        && pre_faces[0][0].wire.is_boundary()
    {
        let mut outline_map = HashMap::default();
        Some(
            face.absolute_boundaries()
                .iter()
                .filter_map(|wire| {
                    create_parameter_boundary(face, wire, &mut outline_map, tol)
                })
                .map(|poly| poly.area())
                .filter(|area| *area > 0.0)
                .sum::<f64>(),
        )
    } else {
        None
    };
    let vec: Vec<_> = pre_faces
        .into_iter()
        .filter(|pre_face| !pre_face.is_empty())
        .map(|pre_face| {
            let surface = face.surface();
            let op = pre_face
                .iter()
                .find(|chunk| chunk.wire.status() != ShapesOpStatus::Unknown);
            let status = match op {
                Some(chunk) => {
                    // PATCH(BrepSCAD): a single pre-face whose area equals the
                    // face's own outline means the intersection loop ran along
                    // the boundary instead of dividing the face. Its And/Or
                    // status is then unreliable, so let the ray cast decide.
                    let boundary_only = boundary_only_area.is_some_and(|outline_area| {
                        let delta = (pre_face[0].poly.area() - outline_area).abs();
                        delta <= 1.0e-9 * outline_area.abs().max(1.0)
                    });
                    if boundary_only {
                        ShapesOpStatus::Unknown
                    } else {
                        chunk.wire.status()
                    }
                }
                None => ShapesOpStatus::Unknown,
            };
            let wires: Vec<Wire<Point3, C>> = pre_face
                .into_iter()
                .map(|chunk| chunk.wire.deref().clone())
                .collect();
            // PATCH(BrepSCAD): boolean-generated wires can be non-simple.
            let mut new_face = match Face::try_new(wires.clone(), surface.clone()) {
                Ok(face) => face,
                Err(_) => Face::new_unchecked(wires, surface),
            };
            if !face.orientation() {
                new_face.invert();
            }
            Some((new_face, status))
        })
        .collect::<Option<Vec<_>>>()?;
    if vec.is_empty() && std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
        eprintln!("TRACE divide: a face produced no pre-faces");
    }
    Some(vec)
}

pub(super) fn divide_faces<C, S>(
    shell: &Shell<Point3, C, S>,
    loops_store: &LoopsStore<Point3, C>,
    tol: f64,
) -> Option<FacesClassification<Point3, C, S>>
where
    C: BoundedCurve<Point = Point3> + ParameterDivision1D<Point = Point3> + Send + Sync,
    S: Clone
        + SearchParameter<SurfaceParameter, Point = Point3>
        + ParametricSurface3D
        + Send
        + Sync,
{
    let mut res = FacesClassification::<Point3, C, S>::default();
    let divide = |(face, loops): (&Face<Point3, C, S>, &Loops<Point3, C>)| -> Option<Vec<FaceWithShapesOpStatus<C, S>>> {
        if loops
            .iter()
            .all(|wire| wire.status() == ShapesOpStatus::Unknown)
        {
            Some(vec![(face.clone(), ShapesOpStatus::Unknown)])
        } else {
            divide_one_face(face, loops, tol)
        }
    };
    let pairs: Vec<_> = shell.iter().zip(loops_store.iter()).collect();
    #[cfg(not(target_arch = "wasm32"))]
    let divided: Vec<_> = {
        use rayon::prelude::*;
        pairs
            .into_par_iter()
            .map(divide)
            .collect::<Option<Vec<_>>>()?
    };
    #[cfg(target_arch = "wasm32")]
    let divided: Vec<_> = pairs.into_iter().map(divide).collect::<Option<Vec<_>>>()?;
    divided
        .into_iter()
        .flatten()
        .for_each(|(face, status)| res.push(face, status));
    Some(res)
}
