//! Evaluated node tree and selectable-part collection.

use std::collections::BTreeMap;
use std::ops::Range;

use brepscad_kernel_api::{Kernel, Mesh, ShapeId, Transform};

use crate::EvalError;
use crate::cache::{MeshCache, MeshKey};
use crate::model::{Parameter, Part, TreeNode};
use crate::profile::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dimension {
    TwoD,
    ThreeD,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) shape: ShapeId,
    pub(crate) module: String,
    pub(crate) source_range: Range<usize>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) transform_parameters: Option<Vec<Parameter>>,
    pub(crate) world: Transform,
    pub(crate) dimension: Dimension,
    pub(crate) profile: Option<Profile>,
    pub(crate) children: Vec<Node>,
}

struct PartContext<'a, K: Kernel> {
    kernel: &'a mut K,
    root_shape: ShapeId,
    root_mesh: &'a Mesh,
    tolerance: f64,
    cache: &'a mut MeshCache,
    /// Build an analytic scene per leaf, for exact picking.
    analytic: bool,
}

pub(crate) fn collect_parts<K: Kernel>(
    node: &Node,
    root_shape: ShapeId,
    root_mesh: &Mesh,
    kernel: &mut K,
    tolerance: f64,
    cache: &mut MeshCache,
    analytic: bool,
) -> Result<(Vec<Part>, TreeNode), EvalError> {
    let mut parts = Vec::new();
    let mut context = PartContext {
        kernel,
        root_shape,
        root_mesh,
        tolerance,
        cache,
        analytic,
    };
    let tree = collect_parts_inner(node, &mut context, &BTreeMap::new(), &mut parts)?;
    Ok((parts, tree))
}

fn collect_parts_inner<K: Kernel>(
    node: &Node,
    context: &mut PartContext<'_, K>,
    inherited: &BTreeMap<usize, Parameter>,
    parts: &mut Vec<Part>,
) -> Result<TreeNode, EvalError> {
    let mut scope = inherited.clone();
    if let Some(transform_parameters) = &node.transform_parameters {
        for parameter in transform_parameters {
            scope
                .entry(parameter.range.start)
                .or_insert_with(|| parameter.clone());
        }
    }

    let mesh_children: Vec<&Node> = node
        .children
        .iter()
        .filter(|child| child.dimension == Dimension::ThreeD)
        .collect();

    if mesh_children.is_empty() {
        for parameter in &node.parameters {
            scope
                .entry(parameter.range.start)
                .or_insert_with(|| parameter.clone());
        }
        let parameters: Vec<Parameter> = scope.into_values().collect();
        let mesh = if node.shape == context.root_shape {
            context.root_mesh.clone()
        } else {
            let key = MeshKey::new(&node.module, &parameters, node.world, context.tolerance);
            match context.cache.get(&key) {
                Some(mesh) => mesh,
                None => {
                    let mesh = context.kernel.tessellate(node.shape, context.tolerance)?;
                    context.cache.insert(key, mesh.clone());
                    mesh
                }
            }
        };
        let part_index = parts.len();
        let scene = if context.analytic {
            context.kernel.raycast_scene(node.shape, context.tolerance)
        } else {
            None
        };
        parts.push(Part {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            parameters,
            mesh,
            scene,
        });
        Ok(TreeNode {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            children: Vec::new(),
            part_index: Some(part_index),
        })
    } else {
        let mut children = Vec::new();
        for child in mesh_children {
            children.push(collect_parts_inner(child, context, &scope, parts)?);
        }
        Ok(TreeNode {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            children,
            part_index: None,
        })
    }
}
