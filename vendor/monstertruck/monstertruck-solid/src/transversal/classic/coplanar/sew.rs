//! Merging the coincident vertices and edges two boolean operands leave behind.

use super::super::super::integrate::{ShapeOpsCurve, ShapeOpsSurface};
use crate::diagnostics;
use monstertruck_geometry::PRESEARCH_DIVISION;
use monstertruck_geometry::prelude::*;
use monstertruck_topology::compress::{CompressedEdge, CompressedEdgeIndex, CompressedShell};
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
    let mut vertices: Vec<Point3> = Vec::new();
    let vertex_map: Vec<usize> = compressed
        .vertices
        .iter()
        .map(|p| {
            vertices
                .iter()
                .position(|q| q.distance(*p) < tol)
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
    let extract_start = debug.then(Instant::now);
    let extracted = Shell::extract(compressed);
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
                if known
                    .curve
                    .search_nearest_parameter(m, Some(hint), 30)
                    .is_some_and(|t| known.curve.subs(t).distance(m) < tol)
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
