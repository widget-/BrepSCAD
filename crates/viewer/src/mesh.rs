//! Combining evaluated shapes and operands into render/pick meshes.

use brepscad_eval::Evaluation;
use brepscad_kernel_api::Mesh;

use crate::picking::PickPart;

pub(crate) fn combine_evaluation(evaluation: &Evaluation) -> Mesh {
    let mut combined = Mesh::default();
    // Normals are only useful when every input mesh has them; a partial set
    // cannot be aligned with the concatenated positions.
    let mut all_normals = true;

    for shape in &evaluation.shapes {
        let offset = combined.positions.len() as u32;
        combined.positions.extend_from_slice(&shape.mesh.positions);
        combined
            .indices
            .extend(shape.mesh.indices.iter().map(|index| index + offset));
        if all_normals && shape.mesh.has_normals() {
            combined.normals.extend_from_slice(&shape.mesh.normals);
        } else if all_normals {
            all_normals = false;
            combined.normals.clear();
        }
    }

    combined
}

pub(crate) fn combine_parts(evaluation: &Evaluation) -> (Mesh, Vec<PickPart>) {
    let mut combined = Mesh::default();
    let mut parts = Vec::new();
    let mut all_normals = true;

    for (shape_index, shape) in evaluation.shapes.iter().enumerate() {
        for (part_index, part) in shape.parts.iter().enumerate() {
            let triangle_start = combined.indices.len() / 3;
            let offset = combined.positions.len() as u32;
            combined.positions.extend_from_slice(&part.mesh.positions);
            combined
                .indices
                .extend(part.mesh.indices.iter().map(|index| index + offset));
            if all_normals && part.mesh.has_normals() {
                combined.normals.extend_from_slice(&part.mesh.normals);
            } else if all_normals {
                all_normals = false;
                combined.normals.clear();
            }
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
    use brepscad_eval::{EvaluatedShape, Part, TreeNode};

    fn triangle_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        }
    }

    fn normal_mesh(normal: [f64; 3]) -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![normal; 3],
            indices: vec![0, 1, 2],
        }
    }

    fn evaluation_with_meshes(shapes: Vec<Mesh>, parts: Vec<Mesh>) -> Evaluation {
        Evaluation {
            shapes: vec![EvaluatedShape {
                mesh: shapes[0].clone(),
                source_range: 0..1,
                parameters: Vec::new(),
                parts: parts
                    .into_iter()
                    .map(|mesh| Part {
                        module: "cube".to_owned(),
                        source_range: 0..1,
                        parameters: Vec::new(),
                        mesh,
                        scene: None,
                    })
                    .collect(),
                tree: TreeNode {
                    module: "difference".to_owned(),
                    source_range: 0..1,
                    children: Vec::new(),
                    part_index: None,
                },
            }],
            messages: Vec::new(),
            scene: None,
        }
    }

    #[test]
    fn combining_keeps_normals_when_every_mesh_has_them() {
        let evaluation = evaluation_with_meshes(
            vec![normal_mesh([0.0, 0.0, 1.0])],
            vec![normal_mesh([0.0, 0.0, 1.0]), normal_mesh([1.0, 0.0, 0.0])],
        );

        let mesh = combine_evaluation(&evaluation);
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert_eq!(mesh.normals, vec![[0.0, 0.0, 1.0]; 3]);

        let (pick_mesh, _) = combine_parts(&evaluation);
        assert_eq!(pick_mesh.normals.len(), pick_mesh.positions.len());
        assert_eq!(&pick_mesh.normals[..3], &[[0.0, 0.0, 1.0]; 3]);
        assert_eq!(&pick_mesh.normals[3..], &[[1.0, 0.0, 0.0]; 3]);
    }

    #[test]
    fn combining_drops_normals_when_one_mesh_lacks_them() {
        let evaluation = evaluation_with_meshes(
            vec![normal_mesh([0.0, 0.0, 1.0])],
            vec![normal_mesh([0.0, 0.0, 1.0]), triangle_mesh()],
        );

        let (pick_mesh, _) = combine_parts(&evaluation);
        assert!(pick_mesh.normals.is_empty());
        assert!(!pick_mesh.has_normals());
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
                        scene: None,
                    },
                    Part {
                        module: "cylinder".to_owned(),
                        source_range: 2..3,
                        parameters: Vec::new(),
                        mesh: triangle_mesh(),
                        scene: None,
                    },
                ],
                tree: TreeNode {
                    module: "difference".to_owned(),
                    source_range: 0..1,
                    children: Vec::new(),
                    part_index: None,
                },
            }],
            messages: Vec::new(),
            scene: None,
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
