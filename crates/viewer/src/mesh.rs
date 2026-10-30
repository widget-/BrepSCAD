//! Combining evaluated shapes and operands into render/pick meshes.

use visual_openscad_kernel_api::Mesh;
use visual_openscad_scad_eval::Evaluation;

use crate::picking::PickPart;

pub(crate) fn combine_evaluation(evaluation: &Evaluation) -> Mesh {
    let mut combined = Mesh::default();

    for shape in &evaluation.shapes {
        let offset = combined.positions.len() as u32;
        combined.positions.extend_from_slice(&shape.mesh.positions);
        combined
            .indices
            .extend(shape.mesh.indices.iter().map(|index| index + offset));
    }

    combined
}

pub(crate) fn combine_parts(evaluation: &Evaluation) -> (Mesh, Vec<PickPart>) {
    let mut combined = Mesh::default();
    let mut parts = Vec::new();

    for (shape_index, shape) in evaluation.shapes.iter().enumerate() {
        for (part_index, part) in shape.parts.iter().enumerate() {
            let triangle_start = combined.indices.len() / 3;
            let offset = combined.positions.len() as u32;
            combined.positions.extend_from_slice(&part.mesh.positions);
            combined
                .indices
                .extend(part.mesh.indices.iter().map(|index| index + offset));
            parts.push(PickPart {
                shape_index,
                part_index,
                triangle_start,
                triangle_count: part.mesh.triangle_count(),
            });
        }
    }

    (combined, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use visual_openscad_scad_eval::{EvaluatedShape, Part, TreeNode};

    fn triangle_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn combine_parts_offsets_indices_and_tracks_operands() {
        let evaluation = Evaluation {
            shapes: vec![EvaluatedShape {
                mesh: triangle_mesh(),
                source_range: 0..1,
                parameters: Vec::new(),
                parts: vec![
                    Part {
                        module: "cube".to_owned(),
                        source_range: 0..1,
                        parameters: Vec::new(),
                        mesh: triangle_mesh(),
                    },
                    Part {
                        module: "cylinder".to_owned(),
                        source_range: 2..3,
                        parameters: Vec::new(),
                        mesh: triangle_mesh(),
                    },
                ],
                tree: TreeNode {
                    module: "difference".to_owned(),
                    source_range: 0..1,
                    children: Vec::new(),
                    part_index: None,
                },
            }],
        };

        let mesh = combine_evaluation(&evaluation);
        assert_eq!(mesh.indices, vec![0, 1, 2]);

        let (pick_mesh, parts) = combine_parts(&evaluation);
        assert_eq!(pick_mesh.indices, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].shape_index, 0);
        assert_eq!(parts[0].part_index, 0);
        assert_eq!(parts[0].triangle_start, 0);
        assert_eq!(parts[1].shape_index, 0);
        assert_eq!(parts[1].part_index, 1);
        assert_eq!(parts[1].triangle_start, 1);
        assert_eq!(parts[1].triangle_count, 1);
    }
}
