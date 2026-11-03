//! Merging the coincident vertices and edges two boolean operands leave behind.

use super::super::super::integrate::{ShapeOpsCurve, ShapeOpsSurface};
use monstertruck_geometry::PRESEARCH_DIVISION;
use monstertruck_geometry::prelude::*;
use monstertruck_topology::compress::{CompressedEdge, CompressedEdgeIndex, CompressedShell};
use monstertruck_topology::*;
use monstertruck_traits::algo::curve::{PresearchSamples, presearch_samples};
use std::collections::HashSet;

pub(in super::super) fn sew<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<Shell<Point3, C, S>> {
    let compressed = shell.compress();
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
    merge_edges(&mut compressed, tol);
    split_boundary_edges(&mut compressed, tol);
    merge_edges(&mut compressed, tol);
    Shell::extract(compressed).ok()
}

fn merge_edges<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    compressed: &mut CompressedShell<Point3, C, S>,
    tol: f64,
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
                let samples = caches[k].get_or_insert_with(|| {
                    presearch_samples(&known.curve, known.curve.range_tuple(), PRESEARCH_DIVISION)
                });
                let hint = samples.hint(m);
                if known
                    .curve
                    .search_nearest_parameter(m, Some(hint), 30)
                    .is_some_and(|t| known.curve.subs(t).distance(m) < tol)
                {
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
) {
    for _ in 0..1000 {
        let mut uses = vec![0usize; compressed.edges.len()];
        for face in &compressed.faces {
            for wire in &face.boundaries {
                for use_ in wire {
                    uses[use_.index] += 1;
                }
            }
        }
        let boundary: Vec<usize> = uses
            .iter()
            .enumerate()
            .filter(|(_, count)| **count == 1)
            .map(|(index, _)| index)
            .collect();
        if boundary.is_empty() {
            return;
        }
        let mut boundary_vertices = HashSet::new();
        for &index in &boundary {
            let (a, b) = compressed.edges[index].vertices;
            boundary_vertices.insert(a);
            boundary_vertices.insert(b);
        }
        let mut split = None;
        'outer: for &index in &boundary {
            let edge = &compressed.edges[index];
            let (a, b) = edge.vertices;
            let (t0, t1) = edge.curve.range_tuple();
            let margin = (t1 - t0) * 1.0e-9;
            let samples = presearch_samples(&edge.curve, (t0, t1), PRESEARCH_DIVISION);
            for &vertex in &boundary_vertices {
                if vertex == a || vertex == b {
                    continue;
                }
                let point = compressed.vertices[vertex];
                let hint = samples.hint(point);
                let Some(t) = edge.curve.search_nearest_parameter(point, Some(hint), 30) else {
                    continue;
                };
                if edge.curve.subs(t).distance(point) > tol {
                    continue;
                }
                if t <= t0 + margin || t >= t1 - margin {
                    continue;
                }
                split = Some((index, vertex, t));
                break 'outer;
            }
        }
        let Some((index, vertex, t)) = split else {
            return;
        };
        let edge = compressed.edges[index].clone();
        let (a, b) = edge.vertices;
        let mut first = edge.curve.clone();
        let second = first.cut(t);
        let first_index = compressed.edges.len();
        compressed.edges.push(CompressedEdge {
            vertices: (a, vertex),
            curve: first,
        });
        let second_index = compressed.edges.len();
        compressed.edges.push(CompressedEdge {
            vertices: (vertex, b),
            curve: second,
        });
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
    }
}
