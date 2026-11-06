//! Merging the coincident vertices and edges two boolean operands leave behind.

use super::super::super::integrate::{ShapeOpsCurve, ShapeOpsSurface};
use crate::diagnostics;
use monstertruck_geometry::PRESEARCH_DIVISION;
use monstertruck_geometry::prelude::*;
use monstertruck_topology::compress::{
    CompressedEdge, CompressedEdgeIndex, CompressedFace, CompressedShell,
};
use monstertruck_topology::*;
use monstertruck_traits::algo::curve::{PresearchSamples, presearch_samples};
use std::collections::HashSet;
use std::time::Instant;

// PATCH(BrepSCAD): opt-in sew state counters for the boolean diagnostics.
#[derive(Default)]
struct SewStats {
    input_vertices: usize,
    input_edges: usize,
    input_faces: usize,
    welded_vertices: usize,
    merge_candidates: usize,
    merge_successes: usize,
    cache_builds: usize,
    removed_edges: usize,
    split_iterations: usize,
    boundary_edges: usize,
    split_candidates: usize,
    splits: usize,
    out_vertices: usize,
    out_edges: usize,
    out_faces: usize,
    left_boundary: usize,
    extract_ok: bool,
    vertex_ms: f64,
    merge_ms: f64,
    split_ms: f64,
    extract_ms: f64,
    total_ms: f64,
}

impl SewStats {
    fn summary(&self) -> String {
        format!(
            "in(v={} e={} f={}) weld(v={}) merge(cand={} ok={} cache={} removed={}) \
             split(iter={} boundary={} checks={} applied={}) out(v={} e={} f={}) \
             left(boundary={} extract_ok={}) \
             time(weld={:.3} merge={:.3} split={:.3} extract={:.3} total={:.3})ms",
            self.input_vertices,
            self.input_edges,
            self.input_faces,
            self.welded_vertices,
            self.merge_candidates,
            self.merge_successes,
            self.cache_builds,
            self.removed_edges,
            self.split_iterations,
            self.boundary_edges,
            self.split_candidates,
            self.splits,
            self.out_vertices,
            self.out_edges,
            self.out_faces,
            self.left_boundary,
            self.extract_ok,
            self.vertex_ms,
            self.merge_ms,
            self.split_ms,
            self.extract_ms,
            self.total_ms,
        )
    }
}

/// Cached samples plus a conservative curve AABB for split candidate filtering.
struct SplitProbe {
    samples: PresearchSamples<Point3>,
    bounds: BoundingBox<Point3>,
}

pub(in super::super) fn sew<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<Shell<Point3, C, S>> {
    let debug = diagnostics::sew_enabled();
    let started = debug.then(Instant::now);
    let compressed = shell.compress();
    if diagnostics::sew_enabled() {
        // Reported before the vertex weld, so a defect here is inherited from
        // whatever built the shell rather than caused by sewing.
        report_wire_defects("raw", &compressed);
    }
    let mut stats = debug.then(|| SewStats {
        input_vertices: compressed.vertices.len(),
        input_edges: compressed.edges.len(),
        input_faces: compressed.faces.len(),
        ..Default::default()
    });
    // Coincidence of vertices is a `confusion` question, not a modelling one.
    // Welding within `tol` merged vertices that are merely close, which pinches
    // a wire through itself (the `NotSimpleWire` class) and collapses edges
    // (the `SameVertex` class).
    let weld = Precision::from_modelling(tol).confusion;
    let mut vertices: Vec<Point3> = Vec::new();
    let vertex_map: Vec<usize> = compressed
        .vertices
        .iter()
        .map(|p| {
            vertices
                .iter()
                .position(|q| q.distance(*p) < weld)
                .unwrap_or_else(|| {
                    vertices.push(*p);
                    vertices.len() - 1
                })
        })
        .collect();
    if let Some(stats) = stats.as_mut() {
        stats.welded_vertices = vertices.len();
    }
    let edges = compressed
        .edges
        .into_iter()
        .map(|edge| {
            let (a, b) = (vertex_map[edge.vertices.0], vertex_map[edge.vertices.1]);
            CompressedEdge {
                vertices: (a, b),
                curve: edge.curve,
            }
        })
        .collect();
    let mut compressed = CompressedShell {
        vertices,
        edges,
        faces: compressed.faces,
        vertex_stable_ids: None,
        edge_stable_ids: None,
        face_stable_ids: None,
    };
    if let (Some(stats), Some(start)) = (stats.as_mut(), started) {
        stats.vertex_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    if debug {
        report_wire_defects("weld", &compressed);
    }
    let merge_start = debug.then(Instant::now);
    merge_edges(&mut compressed, tol, &mut stats);
    if let (Some(stats), Some(start)) = (stats.as_mut(), merge_start) {
        stats.merge_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    if debug {
        report_wire_defects("merge1", &compressed);
    }
    let split_start = debug.then(Instant::now);
    split_boundary_edges(&mut compressed, tol, &mut stats);
    if let (Some(stats), Some(start)) = (stats.as_mut(), split_start) {
        stats.split_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    if debug {
        report_wire_defects("split", &compressed);
    }
    let merge_start = debug.then(Instant::now);
    merge_edges(&mut compressed, tol, &mut stats);
    if let (Some(stats), Some(start)) = (stats.as_mut(), merge_start) {
        stats.merge_ms += start.elapsed().as_secs_f64() * 1000.0;
    }
    if debug {
        report_wire_defects("merge2", &compressed);
    }
    // PATCH(BrepSCAD): a sub-tolerance fragment welds both endpoints onto one
    // vertex, and `Shell::extract` rejects the whole shell as `SameVertex`.
    // The fragment encloses no area and its neighbours meet at the welded
    // vertex, so removing it from the wire keeps the wire closed.
    drop_collapsed_edges(&mut compressed);
    // PATCH(BrepSCAD): a wire that runs out along an edge and straight back
    // along a duplicate revisits its turn vertex, and `Shell::extract` rejects
    // the whole shell as `NotSimpleWire`. The excursion encloses no area, so
    // dropping the pair leaves the region unchanged and the wire simple.
    drop_wire_spikes(&mut compressed);
    // PATCH(BrepSCAD): a wire that passes through a vertex twice is a pinch
    // point, not a defect in the region. `Shell::extract` rejects the whole
    // shell as `NotSimpleWire`; splitting the inner run out as its own loop
    // keeps every face.
    split_wire_at_repeats(&mut compressed, tol);
    let out_vertices = compressed.vertices.len();
    let out_edges = compressed.edges.len();
    let out_faces = compressed.faces.len();
    let mut uses = vec![0usize; compressed.edges.len()];
    for face in &compressed.faces {
        for wire in &face.boundaries {
            for use_ in wire {
                uses[use_.index] += 1;
            }
        }
    }
    let left_boundary = uses.iter().filter(|count| **count == 1).count();
    if debug {
        report_wire_defects("final", &compressed);
    }
    // Provenance: record the outcome against the *input* faces, since
    // `Shell::extract` consumes the compressed shell. A face that disappears
    // from the result can then be found by its key and its reason read off.
    #[cfg(feature = "provenance")]
    let (input_keys, shell_bounds) = {
        let keys: Vec<usize> = shell
            .face_iter()
            .map(|face| face.provenance_key())
            .collect();
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for point in &compressed.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        (keys, (min, max))
    };
    let extract_start = debug.then(Instant::now);
    let extracted = Shell::extract(compressed);
    #[cfg(feature = "provenance")]
    {
        use monstertruck_core::provenance;
        let (min, max) = shell_bounds;
        let outcome = match &extracted {
            Ok(_) => format!(
                "kept, {left_boundary} boundary edges, bounds {min:?}..{max:?}"
            ),
            Err(error) => format!(
                "DROPPED, extract failed {error:?}, {left_boundary} boundary edges, \
                 bounds {min:?}..{max:?}"
            ),
        };
        for key in &input_keys {
            provenance::record(*key, "sew", outcome.clone(), &[]);
        }
    }
    if debug {
        if let Err(error) = &extracted {
            eprintln!("[sew] extract failed: {error:?}");
        }
    }
    let result = extracted.ok();
    if let (Some(stats), Some(start)) = (stats.as_mut(), extract_start) {
        stats.extract_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    if let Some(mut stats) = stats {
        stats.out_vertices = out_vertices;
        stats.out_edges = out_edges;
        stats.out_faces = out_faces;
        stats.left_boundary = left_boundary;
        stats.extract_ok = result.is_some();
        stats.total_ms = started.map_or(0.0, |start| start.elapsed().as_secs_f64() * 1000.0);
        eprintln!("[sew] {}", stats.summary());
    }
    result
}

/// Opt-in dump of the wires `Shell::extract` is about to reject, grouped by the
/// error it will raise, so each class can be attacked on its own.
/// Removes zero-area spikes from every face wire.
///
/// A spike is an edge followed by its own reverse: the wire runs out along one
/// edge and straight back along a duplicate, enclosing nothing. `Shell::extract`
/// rejects such a wire as `NotSimpleWire` because the turn vertex appears twice,
/// which discards the whole shell - and with it every face in it. Dropping the
/// pair cannot change the enclosed region, since the excursion has no area.
fn drop_wire_spikes<C, S>(compressed: &mut CompressedShell<Point3, C, S>) {
    let endpoints: Vec<(usize, usize)> = compressed
        .edges
        .iter()
        .map(|edge| (edge.vertices.0, edge.vertices.1))
        .collect();
    let directed = |use_: &CompressedEdgeIndex| -> (usize, usize) {
        let (start, end) = endpoints[use_.index];
        match use_.orientation {
            true => (start, end),
            false => (end, start),
        }
    };
    for face in &mut compressed.faces {
        for wire in &mut face.boundaries {
            let mut changed = true;
            while changed {
                changed = false;
                let count = wire.len();
                if count < 2 {
                    break;
                }
                for index in 0..count {
                    let next = (index + 1) % count;
                    let (a0, a1) = directed(&wire[index]);
                    let (b0, b1) = directed(&wire[next]);
                    if a0 == b1 && a1 == b0 {
                        let (low, high) = match index < next {
                            true => (index, next),
                            false => (next, index),
                        };
                        wire.remove(high);
                        wire.remove(low);
                        changed = true;
                        break;
                    }
                }
            }
        }
    }
}

/// Splits a wire that revisits a vertex into separate faces.
///
/// A wire like `12 13 17 15 14 6 0 14 12` passes through vertex 14 twice, and
/// the run between the two visits closes on itself. `Shell::extract` rejects
/// such a wire as `NotSimpleWire` and the whole shell is lost with every face in
/// it. Unlike a zero-area spike the inner run does enclose area, so it is kept.
///
/// It has to become its own *face*, not an extra boundary of this one:
/// `Face::try_new` also rejects a face whose wires share a vertex
/// (`Wire::disjoint_wires`), and a pinch point is precisely two loops meeting at
/// one. Two faces sharing that vertex is the representation the topology allows,
/// and it is what the region actually is.
fn split_wire_at_repeats<C, S: Clone>(compressed: &mut CompressedShell<Point3, C, S>, tol: f64) {
    let endpoints: Vec<(usize, usize)> = compressed
        .edges
        .iter()
        .map(|edge| (edge.vertices.0, edge.vertices.1))
        .collect();
    let start_of = |use_: &CompressedEdgeIndex| -> usize {
        let (start, end) = endpoints[use_.index];
        match use_.orientation {
            true => start,
            false => end,
        }
    };
    let mut added: Vec<CompressedFace<S>> = Vec::new();
    let positions: Vec<Point3> = compressed.vertices.clone();
    // Newell's method: the normal of the polygon a loop traces, in the loop's own
    // order. Comparing the inner run's normal with the outer wire's says whether
    // the two wind the same way, which is what decides the new face's
    // orientation - a blanket flip is wrong, since it depends on the pinch.
    let loop_normal = |wire: &[CompressedEdgeIndex]| -> Vector3 {
        let count = wire.len();
        let mut normal = Vector3::new(0.0, 0.0, 0.0);
        for k in 0..count {
            let a = positions[start_of(&wire[k])];
            let b = positions[start_of(&wire[(k + 1) % count])];
            normal.x += (a.y - b.y) * (a.z + b.z);
            normal.y += (a.z - b.z) * (a.x + b.x);
            normal.z += (a.x - b.x) * (a.y + b.y);
        }
        normal
    };
    for (face_index, face) in compressed.faces.iter_mut().enumerate() {
        let mut index = 0;
        while index < face.boundaries.len() {
            let mut split = None;
            let mut seen: Vec<(usize, usize)> = Vec::new();
            for (k, use_) in face.boundaries[index].iter().enumerate() {
                let start = start_of(use_);
                match seen.iter().find(|(vertex, _)| *vertex == start) {
                    Some((_, first)) => {
                        split = Some((*first, k));
                        break;
                    }
                    None => seen.push((start, k)),
                }
            }
            let Some((first, again)) = split else {
                index += 1;
                continue;
            };
            // `first..again` starts and ends on the same vertex, so it is a
            // closed loop on its own. Re-examine the shortened wire afterwards:
            // it may still repeat.
            let inner: Vec<_> = face.boundaries[index].drain(first..again).collect();
            let outer_normal = loop_normal(&face.boundaries[index]);
            let inner_normal = loop_normal(&inner);
            let perimeter = |wire: &[CompressedEdgeIndex]| -> f64 {
                let count = wire.len();
                (0..count)
                    .map(|k| {
                        let a = positions[start_of(&wire[k])];
                        let b = positions[start_of(&wire[(k + 1) % count])];
                        (a - b).magnitude()
                    })
                    .sum()
            };
            // A loop thinner than `tol` encloses nothing: its area is round-off
            // against its own perimeter. Making a face from it adds a degenerate
            // sliver that lies on top of its neighbours, which is what makes two
            // triangles disagree about winding. Drop it, as a spike is dropped.
            if inner_normal.magnitude() * 0.5 <= tol * perimeter(&inner) {
                continue;
            }
            let winds_alike = outer_normal.dot(inner_normal) >= 0.0;
            if diagnostics::sew_enabled() {
                let show = |wire: &[CompressedEdgeIndex]| -> String {
                    wire.iter()
                        .map(|use_| {
                            let point = positions[start_of(use_)];
                            format!("({:.4}, {:.4}, {:.4})", point.x, point.y, point.z)
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                eprintln!(
                    "SPLIT face={face_index} wire={index} at {first}..{again} \
                     winds_alike={winds_alike} face_orientation={}\n  \
                     outer {} -> normal {:?}\n  inner {} -> normal {:?}",
                    face.orientation,
                    show(&face.boundaries[index]),
                    outer_normal,
                    show(&inner),
                    inner_normal
                );
            }
            added.push(CompressedFace {
                boundaries: vec![inner],
                orientation: match winds_alike {
                    true => face.orientation,
                    false => !face.orientation,
                },
                surface: face.surface.clone(),
            });
        }
    }
    compressed.faces.extend(added);
}

/// How many defective wires to dump before going quiet.
///
/// Console noise is the only reason to cap this at all, so the cap is generous
/// and `BREPSCAD_DEBUG_SEW_LIMIT` overrides it.
fn report_limit() -> usize {
    std::env::var("BREPSCAD_DEBUG_SEW_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20)
}

fn report_wire_defects<C, S>(stage: &str, compressed: &CompressedShell<Point3, C, S>) {
    let collapsed: Vec<usize> = compressed
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.vertices.0 == edge.vertices.1)
        .map(|(index, _)| index)
        .collect();
    let mut empty = 0usize;
    let mut open: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    let mut repeated = 0usize;
    for (face_index, face) in compressed.faces.iter().enumerate() {
        for (wire_index, wire) in face.boundaries.iter().enumerate() {
            if wire.is_empty() {
                empty += 1;
                continue;
            }
            let mut broken = Vec::new();
            for k in 0..wire.len() {
                let current = &wire[k];
                let next = &wire[(k + 1) % wire.len()];
                let end = match current.orientation {
                    true => compressed.edges[current.index].vertices.1,
                    false => compressed.edges[current.index].vertices.0,
                };
                let start = match next.orientation {
                    true => compressed.edges[next.index].vertices.0,
                    false => compressed.edges[next.index].vertices.1,
                };
                if end != start {
                    broken.push(k);
                }
            }
            if !broken.is_empty() {
                open.push((face_index, wire_index, broken));
            }
            // A closed wire necessarily ends where it starts, so only the
            // per-edge start vertices are compared.
            let mut seen = HashSet::new();
            let repeats = wire.iter().any(|use_| {
                let edge = &compressed.edges[use_.index];
                let start = match use_.orientation {
                    true => edge.vertices.0,
                    false => edge.vertices.1,
                };
                !seen.insert(start)
            });
            if repeats {
                repeated += 1;
            }
        }
    }
    if collapsed.is_empty() && open.is_empty() && repeated == 0 && empty == 0 {
        return;
    }
    eprintln!(
        "SEW-WIRES[{stage}] edges={} vertices={} collapsed={} empty_wires={empty} open_wires={} repeated_wires={repeated}",
        compressed.edges.len(),
        compressed.vertices.len(),
        collapsed.len(),
        open.len()
    );
    for (face_index, wire_index, broken) in open.iter().take(2) {
        let wire = &compressed.faces[*face_index].boundaries[*wire_index];
        eprintln!("  open face={face_index} wire={wire_index} breaks={broken:?}");
        for (k, use_) in wire.iter().enumerate() {
            let edge = &compressed.edges[use_.index];
            let (start, end) = match use_.orientation {
                true => (edge.vertices.0, edge.vertices.1),
                false => (edge.vertices.1, edge.vertices.0),
            };
            let a = compressed.vertices[start];
            let b = compressed.vertices[end];
            eprintln!(
                "    {k}: e{} {start}->{end} len={:.6} {:?} -> {:?}",
                use_.index,
                (b - a).magnitude(),
                a,
                b
            );
        }
    }
    // A repeated vertex is what `Shell::extract` reports as `NotSimpleWire`.
    // Dump every such wire, up to the noise cap, so a fix can be checked against
    // what remains instead of being hidden behind the first one.
    let mut dumped = 0usize;
    let limit = report_limit();
    'wires: for (face_index, face) in compressed.faces.iter().enumerate() {
        for (wire_index, wire) in face.boundaries.iter().enumerate() {
            let mut seen: Vec<(usize, usize)> = Vec::new();
            let mut repeat = None;
            for (k, use_) in wire.iter().enumerate() {
                let edge = &compressed.edges[use_.index];
                let start = match use_.orientation {
                    true => edge.vertices.0,
                    false => edge.vertices.1,
                };
                match seen.iter().find(|(vertex, _)| *vertex == start) {
                    Some((_, first)) => {
                        repeat = Some((*first, k, start));
                        break;
                    }
                    None => seen.push((start, k)),
                }
            }
            let Some((first, again, vertex)) = repeat else {
                continue;
            };
            eprintln!(
                "  repeated face={face_index} wire={wire_index} len={} vertex={vertex} at {first} and {again} {:?}",
                wire.len(),
                compressed.vertices[vertex]
            );
            for (k, use_) in wire.iter().enumerate() {
                let edge = &compressed.edges[use_.index];
                let (start, end) = match use_.orientation {
                    true => (edge.vertices.0, edge.vertices.1),
                    false => (edge.vertices.1, edge.vertices.0),
                };
                let a = compressed.vertices[start];
                let b = compressed.vertices[end];
                eprintln!(
                    "    {k}: e{} {start}->{end} len={:.6} {:?} -> {:?}",
                    use_.index,
                    (b - a).magnitude(),
                    a,
                    b
                );
            }
            dumped += 1;
            if dumped >= limit {
                eprintln!("  (capped at {limit}; BREPSCAD_DEBUG_SEW_LIMIT raises this)");
                break 'wires;
            }
        }
    }
    for index in collapsed.iter().take(3) {
        let edge = &compressed.edges[*index];
        let point = compressed.vertices[edge.vertices.0];
        eprintln!("  collapsed e{index} at {point:?}");
    }
}

/// Removes edges whose endpoints the vertex weld collapsed onto one vertex.
fn drop_collapsed_edges<C: Clone, S>(compressed: &mut CompressedShell<Point3, C, S>) {
    if !compressed
        .edges
        .iter()
        .any(|edge| edge.vertices.0 == edge.vertices.1)
    {
        return;
    }
    let mut remap = vec![usize::MAX; compressed.edges.len()];
    let mut kept: Vec<CompressedEdge<C>> = Vec::with_capacity(compressed.edges.len());
    for (index, edge) in compressed.edges.iter().enumerate() {
        if edge.vertices.0 != edge.vertices.1 {
            remap[index] = kept.len();
            kept.push(edge.clone());
        }
    }
    for face in &mut compressed.faces {
        for wire in &mut face.boundaries {
            wire.retain(|use_| remap[use_.index] != usize::MAX);
            for use_ in wire.iter_mut() {
                use_.index = remap[use_.index];
            }
        }
        face.boundaries.retain(|wire| !wire.is_empty());
    }
    compressed.faces.retain(|face| !face.boundaries.is_empty());
    compressed.edges = kept;
}

fn merge_edges<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    compressed: &mut CompressedShell<Point3, C, S>,
    tol: f64,
    stats: &mut Option<SewStats>,
) {
    let weld = Precision::from_modelling(tol).confusion;
    let middle = |curve: &C| {
        let (t0, t1) = curve.range_tuple();
        curve.subs((t0 + t1) / 2.0)
    };
    let mut edges: Vec<CompressedEdge<C>> = Vec::new();
    let mut caches: Vec<Option<PresearchSamples<Point3>>> = Vec::new();
    let edge_map: Vec<(usize, bool)> = compressed
        .edges
        .iter()
        .map(|edge| {
            let (a, b) = edge.vertices;
            let m = middle(&edge.curve);
            let mut found = None;
            for (k, known) in edges.iter().enumerate() {
                if known.vertices != (a, b) && known.vertices != (b, a) {
                    continue;
                }
                if let Some(stats) = stats.as_mut() {
                    stats.merge_candidates += 1;
                }
                if caches[k].is_none() {
                    if let Some(stats) = stats.as_mut() {
                        stats.cache_builds += 1;
                    }
                }
                let samples = caches[k].get_or_insert_with(|| {
                    presearch_samples(&known.curve, known.curve.range_tuple(), PRESEARCH_DIVISION)
                });
                let hint = samples.hint(m);
                // Coincidence of two edges is a `confusion` question: they were
                // computed independently, so they agree to rounding, not to the
                // modelling tolerance.
                if known
                    .curve
                    .search_nearest_parameter(m, Some(hint), 30)
                    .is_some_and(|t| known.curve.subs(t).distance(m) < weld)
                {
                    if let Some(stats) = stats.as_mut() {
                        stats.merge_successes += 1;
                    }
                    found = Some((k, known.vertices == (a, b)));
                    break;
                }
            }
            match found {
                Some(index) => index,
                None => {
                    edges.push(CompressedEdge {
                        vertices: (a, b),
                        curve: edge.curve.clone(),
                    });
                    caches.push(None);
                    (edges.len() - 1, true)
                }
            }
        })
        .collect();
    if let Some(stats) = stats.as_mut() {
        stats.removed_edges += compressed.edges.len().saturating_sub(edges.len());
    }
    for face in &mut compressed.faces {
        for wire in &mut face.boundaries {
            for use_ in wire {
                let (index, same) = edge_map[use_.index];
                *use_ = CompressedEdgeIndex {
                    index,
                    orientation: use_.orientation == same,
                };
            }
        }
    }
    compressed.edges = edges;
}

fn split_boundary_edges<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    compressed: &mut CompressedShell<Point3, C, S>,
    tol: f64,
    stats: &mut Option<SewStats>,
) {
    let mut sample_cache: Vec<Option<SplitProbe>> =
        (0..compressed.edges.len()).map(|_| None).collect();
    let mut uses = vec![0usize; compressed.edges.len()];
    for face in &compressed.faces {
        for wire in &face.boundaries {
            for use_ in wire {
                uses[use_.index] += 1;
            }
        }
    }
    let mut boundary: Vec<usize> = uses
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 1)
        .map(|(index, _)| index)
        .collect();
    for _ in 0..1000 {
        if let Some(stats) = stats.as_mut() {
            stats.split_iterations += 1;
        }
        if boundary.is_empty() {
            return;
        }
        if let Some(stats) = stats.as_mut() {
            stats.boundary_edges += boundary.len();
        }
        let mut boundary_vertices = HashSet::new();
        for &index in &boundary {
            let (a, b) = compressed.edges[index].vertices;
            boundary_vertices.insert(a);
            boundary_vertices.insert(b);
        }
        for &index in &boundary {
            if sample_cache[index].is_some() {
                continue;
            }
            let edge = &compressed.edges[index];
            let (t0, t1) = edge.curve.range_tuple();
            // The division polyline is within `tol` of the curve; a vertex
            // within `tol` of the curve must be within `2 * tol` of the
            // polyline. Use a wider margin for floating-point safety.
            let (_, points) = edge.curve.parameter_division((t0, t1), tol);
            let bounds = BoundingBox::from_iter(points);
            let samples = presearch_samples(&edge.curve, (t0, t1), PRESEARCH_DIVISION);
            sample_cache[index] = Some(SplitProbe { samples, bounds });
        }
        let edges = &compressed.edges;
        let vertices = &compressed.vertices;
        let check = |&index: &usize| -> (Option<(usize, usize, f64)>, usize) {
            let edge = &edges[index];
            let (a, b) = edge.vertices;
            let (t0, t1) = edge.curve.range_tuple();
            let margin = (t1 - t0) * 1.0e-9;
            // SAFETY: every boundary edge was populated just above.
            let probe = sample_cache[index].as_ref().unwrap();
            let mut checks = 0;
            let mut edge_split = None;
            for &vertex in &boundary_vertices {
                if vertex == a || vertex == b {
                    continue;
                }
                let point = vertices[vertex];
                let bound_margin = tol * 4.0;
                let min = probe.bounds.min();
                let max = probe.bounds.max();
                if point.x < min.x - bound_margin
                    || point.x > max.x + bound_margin
                    || point.y < min.y - bound_margin
                    || point.y > max.y + bound_margin
                    || point.z < min.z - bound_margin
                    || point.z > max.z + bound_margin
                {
                    continue;
                }
                checks += 1;
                let hint = probe.samples.hint(point);
                let Some(t) = edge.curve.search_nearest_parameter(point, Some(hint), 30) else {
                    continue;
                };
                if edge.curve.subs(t).distance(point) > tol {
                    continue;
                }
                if t <= t0 + margin || t >= t1 - margin {
                    continue;
                }
                edge_split = Some((index, vertex, t));
                break;
            }
            (edge_split, checks)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let candidates: Vec<_> = {
            use rayon::prelude::*;
            boundary.par_iter().map(check).collect()
        };
        #[cfg(target_arch = "wasm32")]
        let candidates: Vec<_> = boundary.iter().map(check).collect();
        let mut splits = Vec::new();
        let mut candidate_checks = 0;
        for (split, checks) in candidates {
            candidate_checks += checks;
            if let Some(split) = split {
                splits.push(split);
            }
        }
        if let Some(stats) = stats.as_mut() {
            stats.split_candidates += candidate_checks;
        }
        if splits.is_empty() {
            return;
        }
        for (index, vertex, t) in splits {
            let old_uses = uses[index];
            if let Some(stats) = stats.as_mut() {
                stats.splits += 1;
            }
            let edge = compressed.edges[index].clone();
            let (a, b) = edge.vertices;
            let mut first = edge.curve.clone();
            let second = first.cut(t);
            let first_index = compressed.edges.len();
            compressed.edges.push(CompressedEdge {
                vertices: (a, vertex),
                curve: first,
            });
            sample_cache.push(None);
            uses.push(0);
            let second_index = compressed.edges.len();
            compressed.edges.push(CompressedEdge {
                vertices: (vertex, b),
                curve: second,
            });
            sample_cache.push(None);
            uses.push(0);
            for face in &mut compressed.faces {
                for wire in &mut face.boundaries {
                    let mut new_wire = Vec::with_capacity(wire.len() + 1);
                    for use_ in wire.drain(..) {
                        if use_.index == index {
                            if use_.orientation {
                                new_wire.push(CompressedEdgeIndex {
                                    index: first_index,
                                    orientation: true,
                                });
                                new_wire.push(CompressedEdgeIndex {
                                    index: second_index,
                                    orientation: true,
                                });
                            } else {
                                new_wire.push(CompressedEdgeIndex {
                                    index: second_index,
                                    orientation: false,
                                });
                                new_wire.push(CompressedEdgeIndex {
                                    index: first_index,
                                    orientation: false,
                                });
                            }
                        } else {
                            new_wire.push(use_);
                        }
                    }
                    *wire = new_wire;
                }
            }
            uses[index] = 0;
            uses[first_index] = old_uses;
            uses[second_index] = old_uses;
            if old_uses == 1 {
                if let Some(position) = boundary.iter().position(|&edge| edge == index) {
                    boundary.remove(position);
                }
                boundary.push(first_index);
                boundary.push(second_index);
            }
        }
    }
}
