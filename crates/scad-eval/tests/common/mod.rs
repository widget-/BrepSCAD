use brepscad_kernel_api::Mesh;

#[allow(dead_code)]
pub fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    (min, max)
}

#[allow(dead_code)]
pub fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum::<f64>()
        / 6.0
}

/// Edges used by exactly one triangle after welding by position.
#[allow(dead_code)]
pub fn welded_boundary_edges(mesh: &Mesh) -> usize {
    use std::collections::HashMap;
    let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let mut indices = [0u32; 3];
        for (slot, index) in triangle.iter().enumerate() {
            let position = mesh.positions[*index as usize];
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
            let edge = (start.min(end), start.max(end));
            *edges.entry(edge).or_default() += 1;
        }
    }
    edges.values().filter(|count| **count == 1).count()
}
