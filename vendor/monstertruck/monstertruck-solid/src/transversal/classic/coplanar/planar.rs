//! Planar-arrangement fallback boolean.
//!
//! PATCH(BrepSCAD): the classic loop assembly cannot represent an intersection
//! curve that collapses to a point, which happens where two near-tangent planar
//! faces meet. This path splits every planar face with the other solid's plane
//! cross-section and classifies the resulting pieces with a point-in-solid
//! test. It is only used when the normal pipeline fails.

use super::super::super::integrate::{ShapeOpsCurve, ShapeOpsSurface};
use super::plane2d::*;
use super::split::{apply_splits, note_split, unique_edges, Splits};
use super::{divide, inner_point, plane_of, PlaneOf};
use monstertruck_geometry::prelude::*;
use monstertruck_meshing::prelude::*;
use monstertruck_topology::*;
use std::collections::{HashMap, HashSet};

type PolyShell = Shell<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>;

#[derive(Clone, Copy)]
pub(super) enum PlanarOp {
    And,
    Or,
}

pub(in crate::transversal::classic) fn and<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    a: &Shell<Point3, C, S>,
    b: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<Shell<Point3, C, S>> {
    if !allowed(a, b, PlanarOp::And) {
        return None;
    }
    pair(a, b, tol, PlanarOp::And)
}

pub(in crate::transversal::classic) fn or<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    a: &Shell<Point3, C, S>,
    b: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<Shell<Point3, C, S>> {
    if !allowed(a, b, PlanarOp::Or) {
        return None;
    }
    let shell = pair(a, b, tol, PlanarOp::Or)?;
    if shell_is_closed(&shell, tol) {
        Some(shell)
    } else {
        if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
            eprintln!("PLANAR: union result is open, rejecting");
        }
        None
    }
}

/// Keep the fallback for cutout-sized tools. Large planar pairs are handled by
/// the normal pipeline (or its existing fallbacks) so this path cannot change
/// unrelated booleans.
fn allowed<C, S: ShapeOpsSurface>(
    a: &Shell<Point3, C, S>,
    b: &Shell<Point3, C, S>,
    op: PlanarOp,
) -> bool {
    let (Some(va), Some(vb)) = (bounds_volume(a), bounds_volume(b)) else {
        return false;
    };
    match op {
        PlanarOp::And => vb <= va * 0.1 && vb <= 1000.0,
        PlanarOp::Or => va.max(vb) <= 1000.0,
    }
}

fn bounds_volume<C, S: ShapeOpsSurface>(shell: &Shell<Point3, C, S>) -> Option<f64> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for face in shell.face_iter() {
        for wire in face.boundaries() {
            for vertex in wire.vertex_iter() {
                let point = vertex.point();
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    if !min.iter().all(|value| value.is_finite()) {
        return None;
    }
    Some((max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2]))
}

fn shell_is_closed<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell: &Shell<Point3, C, S>,
    tol: f64,
) -> bool {
    let poly = shell.triangulation(tol);
    let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    let mut missing = 0usize;
    let mut faces = 0usize;
    for face in poly.face_iter() {
        faces += 1;
        let Some(mesh) = face.surface() else {
            missing += 1;
            continue;
        };
        let positions = mesh.positions();
        for triangle in mesh.faces().triangle_iter() {
            let mut indices = [0u32; 3];
            for (slot, vertex) in triangle.iter().enumerate() {
                let position = positions[vertex.pos];
                let key = (
                    (position[0] * 1.0e5).round() as i64,
                    (position[1] * 1.0e5).round() as i64,
                    (position[2] * 1.0e5).round() as i64,
                );
                let next = welded.len() as u32;
                indices[slot] = *welded.entry(key).or_insert(next);
            }
            for (start, end) in [
                (indices[0], indices[1]),
                (indices[1], indices[2]),
                (indices[2], indices[0]),
            ] {
                let edge = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                *edges.entry(edge).or_default() += 1;
            }
        }
    }
    let open = edges.values().filter(|count| **count == 1).count();
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() && (missing > 0 || open > 0) {
        eprintln!(
            "PLANAR: shell_is_closed=false faces={faces} missing_mesh={missing} open_edges={open} vertices={}",
            welded.len()
        );
    }
    missing == 0 && open == 0
}

fn pair<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    a: &Shell<Point3, C, S>,
    b: &Shell<Point3, C, S>,
    tol: f64,
    op: PlanarOp,
) -> Option<Shell<Point3, C, S>> {
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        eprintln!(
            "PLANAR: entered faces {}x{}",
            a.face_iter().count(),
            b.face_iter().count()
        );
    }
    if !planar(a) || !planar(b) {
        if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
            eprintln!(
                "PLANAR: not all faces planar ({}, {})",
                a.face_iter().count(),
                b.face_iter().count()
            );
        }
        return None;
    }
    let poly_a = a.triangulation(tol);
    let poly_b = b.triangulation(tol);
    let inverted_a = poly_a.to_polygon().volume() < 0.0;
    let inverted_b = poly_b.to_polygon().volume() < 0.0;
    let mut result = Shell::new();
    for face in a.face_iter() {
        result.extend(split_and_keep(face, b, &poly_b, tol, op, inverted_b)?);
    }
    for face in b.face_iter() {
        result.extend(split_and_keep(face, a, &poly_a, tol, op, inverted_a)?);
    }
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        eprintln!("PLANAR: result faces={}", result.face_iter().count());
    }
    if result.face_iter().next().is_none() {
        return None;
    }
    // PATCH(BrepSCAD): each face is divided against the other solid on its own,
    // so a split point on a shared edge exists on one side only and the two
    // faces meet at a T-junction. Sewing splits the neighbour's edge at that
    // vertex, which is what closes the shell.
    let result = super::sew(&result, tol).unwrap_or(result);
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        report_open_edges(&result, tol);
    }
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        let first = result.face_iter().next().unwrap();
        let boundaries = first.boundaries();
        let vertices = boundaries
            .first()
            .map(|wire| wire.vertex_iter().count())
            .unwrap_or(0);
        eprintln!(
            "PLANAR: result faces={} first_boundaries={} first_vertices={}",
            result.face_iter().count(),
            boundaries.len(),
            vertices
        );
    }
    Some(result)
}

fn planar<C, S: ShapeOpsSurface>(shell: &Shell<Point3, C, S>) -> bool {
    shell.face_iter().all(|face| plane_of(face).is_some())
}

/// Classifies the edges of a fallback result that are used only once: a
/// T-junction (some other vertex lies inside the edge), a zero-length edge, a
/// coincident edge that was split differently, or an unexplained hole.
fn report_open_edges<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell: &Shell<Point3, C, S>,
    tol: f64,
) {
    let mut uses: HashMap<EdgeId<C>, usize> = HashMap::new();
    let mut edges: Vec<Edge<Point3, C>> = Vec::new();
    let mut faces = 0;
    for face in shell.face_iter() {
        faces += 1;
        for wire in face.absolute_boundaries() {
            for edge in wire.iter() {
                match uses.get_mut(&edge.id()) {
                    Some(count) => *count += 1,
                    None => {
                        uses.insert(edge.id(), 1);
                        edges.push(edge.absolute_clone());
                    }
                }
            }
        }
    }
    let mut points: Vec<Point3> = Vec::new();
    for edge in &edges {
        for point in [edge.absolute_front().point(), edge.absolute_back().point()] {
            if !points.iter().any(|known| known.distance(point) < tol) {
                points.push(point);
            }
        }
    }
    let open: Vec<&Edge<Point3, C>> = edges
        .iter()
        .filter(|edge| uses.get(&edge.id()).copied().unwrap_or(0) == 1)
        .collect();
    let (mut degenerate, mut tjunction, mut coincident, mut unexplained) = (0, 0, 0, 0);
    for edge in &open {
        let (a, b) = (edge.absolute_front().point(), edge.absolute_back().point());
        if a.distance(b) < tol {
            degenerate += 1;
            continue;
        }
        let along = b - a;
        let inside = points.iter().any(|point| {
            if point.distance(a) < tol || point.distance(b) < tol {
                return false;
            }
            let s = ((*point - a).dot(along) / along.magnitude2().max(1.0e-300)).clamp(0.0, 1.0);
            (a + along * s).distance(*point) < tol
        });
        if inside {
            tjunction += 1;
            continue;
        }
        let paired = open.iter().any(|other| {
            other.id() != edge.id()
                && ((other.absolute_front().point().distance(a) < tol
                    && other.absolute_back().point().distance(b) < tol)
                    || (other.absolute_front().point().distance(b) < tol
                        && other.absolute_back().point().distance(a) < tol))
        });
        if paired {
            coincident += 1;
        } else {
            unexplained += 1;
        }
    }
    eprintln!(
        "PLANAR: open edges={} degenerate={degenerate} tjunction={tjunction} \
         coincident={coincident} unexplained={unexplained} faces={faces} vertices={}",
        open.len(),
        points.len()
    );
}

/// Divides `face` by the curves of `others` with the exact planar arrangement:
/// every curve is split at its mutual intersections, the faces are walked, and
/// the holes are nested.
///
/// Unlike `divide`, the pieces are cut from the curves themselves and meet at
/// shared nodes, so a divided face cannot inherit a wire gap from a sampling
/// tolerance. Returns `None` when the arrangement cannot describe the face, so
/// the caller can fall back to `divide`.
fn divide_arrangement<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    face: &Face<Point3, C, S>,
    others: &[&Face<Point3, C, S>],
    tol: f64,
) -> Option<Vec<Face<Point3, C, S>>> {
    let plane = plane_of(face)?;
    let frame = Frame::new(plane);
    let own_outline = outline(face, &frame, tol);
    let mut curves: Vec<C> = Vec::new();
    for wire in face.boundaries() {
        for edge in wire.edge_iter() {
            curves.push(edge.curve());
        }
    }
    let boundary_count = curves.len();
    let mut seen = HashSet::new();
    for other in others {
        for edge in other.edge_iter() {
            if !seen.insert(edge.id()) {
                continue;
            }
            let sampled = samples(&edge.curve(), tol);
            let flat: Vec<Point2> = sampled.iter().map(|(_, point)| frame.flat(*point)).collect();
            if flat.len() < 2 {
                continue;
            }
            let middle = match flat.len() > 2 {
                true => flat[flat.len() / 2],
                false => flat[0] + (flat[1] - flat[0]) * 0.5,
            };
            if on_outline(&own_outline, middle, tol) || !inside(&own_outline, middle) {
                continue;
            }
            curves.push(edge.curve());
        }
    }
    if curves.len() == boundary_count {
        return Some(vec![face.clone()]);
    }

    let arrangement = arrange(&curves, tol);
    let loops = extract_loops(&arrangement, plane.normal, tol);
    let regions = nest_loops(&arrangement, &loops, plane.normal, tol);
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        let face_area: f64 = own_outline.iter().map(|polygon| polygon_area(polygon)).sum();
        let covered: f64 = loops
            .iter()
            .filter(|item| item.area > tol * tol)
            .map(|item| item.area)
            .sum();
        eprintln!(
            "PLANAR-ARRANGE face={face_area:.4} curves={} pieces={} loops={} regions={} covered={covered:.4}",
            curves.len(),
            arrangement.segments.len(),
            loops.len(),
            regions.len()
        );
    }
    if regions.is_empty() {
        return None;
    }
    let vertices: Vec<Vertex<Point3>> = arrangement
        .nodes
        .iter()
        .map(|point| Vertex::new(*point))
        .collect();
    let surface = face.surface();
    let mut out = Vec::new();
    for region in &regions {
        let mut wires: Vec<Wire<Point3, C>> = Vec::new();
        let mut usable = true;
        for loop_index in std::iter::once(region.outer).chain(region.holes.iter().copied()) {
            let mut edges = Vec::new();
            for &(segment, forward) in &loops[loop_index].pieces {
                let piece = &arrangement.segments[segment];
                if piece.from == piece.to {
                    usable = false;
                    break;
                }
                let curve = piece.curve.clone();
                // The piece's own curve always runs `from -> to`; a backward
                // traversal is the same edge with its orientation flipped, not
                // a rebuilt curve.
                edges.push(match forward {
                    true => Edge::new(&vertices[piece.from], &vertices[piece.to], curve),
                    false => Edge::new(&vertices[piece.from], &vertices[piece.to], curve).inverse(),
                });
            }
            if !usable {
                break;
            }
            wires.push(edges.into());
        }
        if !usable || wires.is_empty() {
            return None;
        }
        let absolute: Vec<Wire<Point3, C>> = match face.orientation() {
            true => wires,
            false => wires.iter().map(Wire::inverse).collect(),
        };
        let mut piece = Face::new_unchecked(absolute, surface.clone());
        if !face.orientation() {
            piece.invert();
        }
        // The pieces are cut from the source curves and meet at intersected
        // points, so they carry the accumulated error of that construction.
        // Recording it lets a later coincidence test judge them against their
        // own budget instead of the modelling tolerance.
        piece.raise_tolerance(Precision::from_modelling(tol).confusion);
        // Provenance: this piece exists because `divide` split the source face
        // into these regions. A wire defect on the piece can be traced back to
        // the face and the arrangement that produced it.
        #[cfg(feature = "provenance")]
        {
            use monstertruck_core::provenance;
            let key = piece.provenance_key();
            provenance::label(key, "piece");
            provenance::record(
                key,
                "divide",
                format!(
                    "region area {:.4} of {} regions, {} pieces, {} curves",
                    loops[region.outer].area,
                    regions.len(),
                    arrangement.segments.len(),
                    curves.len()
                ),
                &[face.provenance_key()],
            );
        }
        out.push(piece);
    }
    Some(out)
}

/// Split one face by the cross-section of `other` on its plane, then keep the
/// pieces on the correct side for the operation.
fn split_and_keep<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    face: &Face<Point3, C, S>,
    other: &Shell<Point3, C, S>,
    other_poly: &PolyShell,
    tol: f64,
    op: PlanarOp,
    other_inverted: bool,
) -> Option<Vec<Face<Point3, C, S>>> {
    let Some(plane) = plane_of(face) else {
        if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
            eprintln!("PLANAR: face has no plane");
        }
        return None;
    };
    let frame = Frame::new(plane);
    let outline = outline(face, &frame, tol);
    let cuts = cross_section_edges(face, &plane, other, &frame, &outline, tol);
    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
        eprintln!("PLANAR: face cuts={}", cuts.len());
    }
    // PATCH(BrepSCAD): `divide` expects the cut endpoints to be vertices of
    // the face boundary. Split the boundary at those points first, otherwise
    // the half-edge graph creates T-junctions and returns None.
    let mut split_face = face.clone();
    if !cuts.is_empty() {
        let mut holder = Shell::new();
        holder.push(face.clone());
        let boundary_edges = unique_edges(&holder);
        let mut splits = Splits::default();
        for cut in &cuts {
            for point in [
                cut.absolute_front().point(),
                cut.absolute_back().point(),
            ] {
                for (index, edge) in boundary_edges.iter().enumerate() {
                    note_split(&mut splits, index, edge, point, tol);
                }
            }
        }
        if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
            eprintln!(
                "PLANAR: cuts={} boundary_edges={} split_edges={} split_points={}",
                cuts.len(),
                boundary_edges.len(),
                splits.len(),
                splits.values().map(Vec::len).sum::<usize>()
            );
        }
        let split_shell = apply_splits(&holder, &boundary_edges, &splits);
        split_face = split_shell.face_iter().next()?.clone();
    }
    let pieces: Vec<Face<Point3, C, S>> = if cuts.is_empty() {
        vec![split_face]
    } else {
        let mut holders = Vec::with_capacity(cuts.len());
        for edge in cuts {
            let wire: Wire<Point3, C> = vec![edge].into();
            holders.push(Face::new_unchecked(
                vec![wire],
                split_face.surface().clone(),
            ));
        }
        let refs: Vec<&Face<Point3, C, S>> = holders.iter().collect();
        let arranged = divide_arrangement(&split_face, &refs, tol);
        if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
            let area_of = |pieces: &[Face<Point3, C, S>]| -> f64 {
                pieces
                    .iter()
                    .map(|piece| {
                        super::plane2d::outline(piece, &frame, tol)
                            .iter()
                            .map(|polygon| polygon_area(polygon))
                            .sum::<f64>()
                    })
                    .sum()
            };
            let old_total = divide(&split_face, &refs, tol)
                .map(|list| area_of(&list.into_iter().map(|(piece, _)| piece).collect::<Vec<_>>()));
            let new_total = arranged.as_ref().map(|list| area_of(list));
            let own: f64 = outline.iter().map(|polygon| polygon_area(polygon)).sum();
            eprintln!(
                "PLANAR-COMPARE face={own:.4} old={old_total:?} new={new_total:?}"
            );
        }
        match arranged {
            Some(pieces) if !pieces.is_empty() => pieces,
            _ => match divide(&split_face, &refs, tol) {
                Some(pieces) => pieces.into_iter().map(|(piece, _)| piece).collect(),
                None => {
                    if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
                        eprintln!("PLANAR: divide failed, keeping whole face");
                    }
                    vec![split_face]
                }
            },
        }
    };

    let keep_inside = matches!(op, PlanarOp::And);
    let debug = std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some();
    if debug {
        let face_area: f64 = outline.iter().map(|polygon| polygon_area(polygon)).sum();
        eprintln!(
            "PLANAR-KEEP face area={face_area:.4} pieces={} op={:?}",
            pieces.len(),
            keep_inside
        );
    }
    let mut kept = Vec::new();
    for piece in pieces {
        let point = match inner_point(&piece, tol) {
            Some(point) => point,
            None => {
                if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
                    eprintln!("PLANAR: inner point fallback");
                }
                let Some(first) = piece
                    .boundaries()
                    .first()
                    .and_then(|wire| wire.vertex_iter().next())
                else {
                    continue;
                };
                first.point()
            }
        };
        let inside = match point_inside(point, other_poly) {
            Some(inside) => {
                if other_inverted {
                    !inside
                } else {
                    inside
                }
            }
            None => {
                if std::env::var_os("BREPSCAD_DEBUG_PLANAR").is_some() {
                    eprintln!("PLANAR: point inside fallback");
                }
                false
            }
        };
        if debug {
            let area: f64 = super::plane2d::outline(&piece, &frame, tol)
                .iter()
                .map(|polygon| polygon_area(polygon))
                .sum();
            eprintln!(
                "  PLANAR-KEEP piece area={area:.4} wires={} edges={} inside={inside} keep={} point={point:?}",
                piece.boundaries().len(),
                piece.edge_iter().count(),
                inside == keep_inside
            );
        }
        if inside == keep_inside {
            kept.push(piece);
        }
    }
    Some(kept)
}

/// The intersection segments of `other`'s triangulation with `face`'s plane,
/// clipped to the face outline and converted back to edges on both surfaces.
fn cross_section_edges<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    face: &Face<Point3, C, S>,
    plane: &PlaneOf,
    other: &Shell<Point3, C, S>,
    frame: &Frame,
    outline: &[Vec<Point2>],
    tol: f64,
) -> Vec<Edge<Point3, C>> {
    let eps = tol * 1.0e-3;
    // The cross-section is clipped against the face's 2D polyline outline, so a
    // clipped endpoint can land as far off the true boundary curve as the
    // polyline deviates from it. `note_split` then refuses to split the
    // boundary at that point (it wants the point within `tol`) and the divided
    // face keeps a wire gap. Pull each endpoint onto the boundary curve first.
    let reach = tol * 4.0;
    let boundary: Vec<Edge<Point3, C>> = face
        .boundaries()
        .iter()
        .flat_map(|wire| wire.edge_iter().cloned().collect::<Vec<_>>())
        .collect();
    let snap_to_boundary = |point: Point3| -> Point3 {
        let mut best: Option<(f64, Point3)> = None;
        for edge in &boundary {
            let curve = edge.curve();
            let Some(t) = curve.search_nearest_parameter(point, None, 100) else {
                continue;
            };
            let foot = curve.subs(t);
            let distance = foot.distance(point);
            if distance <= reach && best.is_none_or(|(known, _)| distance < known) {
                best = Some((distance, foot));
            }
        }
        best.map_or(point, |(_, foot)| foot)
    };
    let mut edges = Vec::new();
    for other_face in other.face_iter() {
        let Some(other_plane) = plane_of(other_face) else {
            continue;
        };
        if other_plane.normal.cross(plane.normal).magnitude() < 1.0e-9 {
            // Parallel or coplanar: this face contributes no cross-section.
            continue;
        }
        for wire in other_face.boundaries() {
            let mut crossings: Vec<Point3> = Vec::new();
            for edge in wire.edge_iter() {
                let points = oriented_points(edge, tol);
                for pair in points.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let (da, db) = (
                        (a - plane.origin).dot(plane.normal),
                        (b - plane.origin).dot(plane.normal),
                    );
                    if da.abs() <= eps {
                        crossings.push(a);
                    }
                    if (da > eps && db < -eps) || (da < -eps && db > eps) {
                        let t = da / (da - db);
                        crossings.push(a + (b - a) * t);
                    }
                }
            }
            crossings.dedup_by(|a, b| a.distance(*b) <= eps);
            for pair in crossings.chunks_exact(2) {
                let (a, b) = (frame.flat(pair[0]), frame.flat(pair[1]));
                for (a, b) in clip_segment(outline, a, b, tol) {
                    let raw = (frame.lift(a), frame.lift(b));
                    // Only endpoints the clip put on the outline may move: an
                    // endpoint that was already strictly inside the face is
                    // where the cut genuinely stops, and moving it would extend
                    // the cut.
                    let snap = |flat: Point2, point: Point3| -> Point3 {
                        if !on_outline(outline, flat, tol * 0.5) {
                            return point;
                        }
                        // Solve the crossing on the boundary curve itself. The
                        // clip produced this endpoint from a chord polyline, so
                        // it can sit up to `tol` off the curve, and `note_split`
                        // then refuses to split the boundary there.
                        let mut best: Option<(f64, Point3)> = None;
                        for edge in &boundary {
                            let curve = edge.curve();
                            for hit in segment_curve_intersections(
                                &curve,
                                curve.range_tuple(),
                                raw.0,
                                raw.1,
                                plane.normal,
                                tol,
                            ) {
                                let distance = hit.point.distance(point);
                                if best.is_none_or(|(known, _)| distance < known) {
                                    best = Some((distance, hit.point));
                                }
                            }
                        }
                        best.map_or_else(|| snap_to_boundary(point), |(_, hit)| hit)
                    };
                    let (p0, p1) = (snap(a, raw.0), snap(b, raw.1));
                    // `tol`, not `eps`: a segment shorter than the vertex weld
                    // collapses to a single vertex in `sew`, and the resulting
                    // zero-length edge makes `Shell::extract` reject the shell.
                    if (p1 - p0).magnitude() <= tol {
                        continue;
                    }
                    let curve = segment_curve(&face.surface(), &other_face.surface(), p0, p1);
                    let (v0, v1) = (Vertex::new(p0), Vertex::new(p1));
                    edges.push(Edge::new(&v0, &v1, curve));
                }
            }
        }
    }
    let mut unique: Vec<Edge<Point3, C>> = Vec::new();
    for edge in edges {
        let (a, b) = (
            edge.absolute_front().point(),
            edge.absolute_back().point(),
        );
        let duplicate = unique.iter().any(|other| {
            let (c, d) = (
                other.absolute_front().point(),
                other.absolute_back().point(),
            );
            (a.distance(c) <= tol && b.distance(d) <= tol)
                || (a.distance(d) <= tol && b.distance(c) <= tol)
        });
        if !duplicate {
            unique.push(edge);
        }
    }
    unique
}

fn clip_segment(
    outline: &[Vec<Point2>],
    a: Point2,
    b: Point2,
    tol: f64,
) -> Vec<(Point2, Point2)> {
    if (b - a).magnitude() <= tol {
        return Vec::new();
    }
    let mut params = vec![0.0, 1.0];
    for polygon in outline {
        let n = polygon.len();
        for i in 0..n {
            let (c, d) = (polygon[i], polygon[(i + 1) % n]);
            if let Some((s, _)) = crossing(a, b, c, d) {
                params.push(s);
            }
        }
    }
    params.sort_by(|left, right| left.total_cmp(right));
    params.dedup_by(|left, right| (*left - *right).abs() < 1.0e-9);
    let mut result = Vec::new();
    for pair in params.windows(2) {
        let (t0, t1) = (pair[0], pair[1]);
        if t1 - t0 <= 1.0e-9 {
            continue;
        }
        let mid = a + (b - a) * ((t0 + t1) * 0.5);
        if inside(outline, mid) && !on_outline(outline, mid, tol * 0.1) {
            result.push((a + (b - a) * t0, a + (b - a) * t1));
        }
    }
    result
}

fn segment_curve<C, S: Clone>(
    surface0: &S,
    surface1: &S,
    p0: Point3,
    p1: Point3,
) -> C
where
    C: From<
        SurfaceCurve<
            BsplineCurve<Point3>,
            S,
            S,
            ParameterCurve<BoundaryCurve2D, S>,
            ParameterCurve<BoundaryCurve2D, S>,
        >,
    >,
{
    let knots = KnotVector::from(vec![0.0, 0.0, 1.0, 1.0]);
    let bspline = BsplineCurve::new(knots, vec![p0, p1]);
    let boundary0: Option<ParameterCurve<BoundaryCurve2D, S>> = None;
    let boundary1: Option<ParameterCurve<BoundaryCurve2D, S>> = None;
    SurfaceCurve::with_boundaries(
        surface0.clone(),
        surface1.clone(),
        bspline,
        boundary0,
        boundary1,
    )
    .into()
}

fn point_inside(point: Point3, poly: &PolyShell) -> Option<bool> {
    let dirs = [
        Vector3::new(1.0, 0.37, 0.23).normalize(),
        Vector3::new(0.23, 1.0, 0.37).normalize(),
        Vector3::new(0.37, 0.23, 1.0).normalize(),
    ];
    let mut votes = 0;
    for dir in dirs {
        let mut count = 0;
        for face in poly.face_iter() {
            let mesh = face.surface()?;
            let crossed = mesh.signed_crossing_faces(point, dir);
            count += if face.orientation() { crossed } else { -crossed };
        }
        if count != 0 {
            votes += 1;
        }
    }
    Some(votes * 2 > dirs.len())
}
