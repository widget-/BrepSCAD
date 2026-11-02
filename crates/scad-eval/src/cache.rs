//! Bounded cache of tessellated leaf meshes.

use std::collections::{HashMap, VecDeque};

use brepscad_kernel_api::{Mesh, Transform};

use crate::model::Parameter;

/// A bounded cache of tessellated leaf meshes, reused across evaluations.
///
/// Entries are keyed by the leaf's module, resolved numeric parameters
/// (including inherited transform parameters), composed world transform, and
/// tessellation tolerance. Editing a parameter re-tessellates only the leaves
/// whose geometry changed.
#[derive(Debug, Default)]
pub struct MeshCache {
    entries: HashMap<MeshKey, Mesh>,
    order: VecDeque<MeshKey>,
    hits: usize,
    misses: usize,
}

const MESH_CACHE_LIMIT: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MeshKey {
    module: String,
    values: Vec<u64>,
    transform: [u64; 16],
    tolerance: u64,
}

impl MeshKey {
    pub(crate) fn new(
        module: &str,
        parameters: &[Parameter],
        world: Transform,
        tolerance: f64,
    ) -> Self {
        let mut transform = [0_u64; 16];
        for (index, value) in world.cols.iter().flatten().enumerate() {
            transform[index] = value.to_bits();
        }
        Self {
            module: module.to_owned(),
            values: parameters
                .iter()
                .map(|parameter| parameter.value.to_bits())
                .collect(),
            transform,
            tolerance: tolerance.to_bits(),
        }
    }
}

impl MeshCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tessellations served from the cache.
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// Number of tessellations performed because the key was absent.
    pub fn misses(&self) -> usize {
        self.misses
    }

    pub(crate) fn get(&mut self, key: &MeshKey) -> Option<Mesh> {
        match self.entries.get(key) {
            Some(mesh) => {
                self.hits += 1;
                Some(mesh.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub(crate) fn insert(&mut self, key: MeshKey, mesh: Mesh) {
        if self.entries.len() >= MESH_CACHE_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, mesh);
    }
}
