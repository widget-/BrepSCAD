//! Evaluator connecting the source tree to a kernel.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use visual_openscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Transform};
use visual_openscad_scad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, parse};

use crate::cache::{MeshCache, MeshKey};
use crate::model::{EvaluatedShape, Evaluation, Parameter, Part, TreeNode};
use crate::parameters::{collect_parameters, first_ident, parse_number, text_range_to_range};
use crate::value::{Value, binary_value, unary_value};
use crate::{EvalError, bad_args};

const SINGULAR_DETERMINANT: f64 = 1.0e-12;

#[derive(Debug, Default)]
struct Arguments {
    positional: Vec<Value>,
    named: HashMap<String, Value>,
}

impl Arguments {
    fn argument(&self, name: &str, index: usize) -> Option<&Value> {
        self.named.get(name).or_else(|| self.positional.get(index))
    }

    fn one(&self, module: &str, name: &str) -> Result<&Value, EvalError> {
        if self.positional.len() > 1 {
            return Err(bad_args(module, "expected one argument"));
        }
        self.argument(name, 0)
            .ok_or_else(|| bad_args(module, "expected one argument"))
    }

    fn ensure_known(&self, module: &str, allowed: &[&str]) -> Result<(), EvalError> {
        for name in self.named.keys() {
            if !allowed.contains(&name.as_str()) {
                return Err(bad_args(
                    module,
                    format!("unsupported named argument {name}"),
                ));
            }
        }
        Ok(())
    }

    fn ensure_empty(&self, module: &str) -> Result<(), EvalError> {
        if self.positional.is_empty() && self.named.is_empty() {
            Ok(())
        } else {
            Err(bad_args(module, "expected no arguments"))
        }
    }
}

/// Parses and evaluates `source`, returning each top-level shape with its
/// source range, numeric parameter bindings, and selectable operands.
pub fn evaluate<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Evaluation, EvalError> {
    evaluate_with_cache(source, kernel, tolerance, &mut MeshCache::default())
}

/// Like [`evaluate`], but reuses leaf meshes from `cache`.
pub fn evaluate_with_cache<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
    cache: &mut MeshCache,
) -> Result<Evaluation, EvalError> {
    let parsed = parse(source);
    if !parsed.errors().is_empty() {
        let messages = parsed
            .errors()
            .iter()
            .map(|error| format!("{} at offset {}", error.message, error.offset))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(EvalError::Parse(messages));
    }

    let mut roots = Vec::new();
    {
        let mut evaluator = Evaluator {
            kernel,
            variables: HashMap::new(),
            assignments: HashMap::new(),
            tolerance,
        };

        for statement in parsed.syntax().children() {
            if let Some(node) = evaluator.statement(statement, Transform::IDENTITY)? {
                roots.push(node);
            }
        }
    }

    let mut shapes = Vec::new();
    for root in roots {
        let mesh = kernel.tessellate(root.shape, tolerance)?;
        let (parts, tree) = collect_parts(&root, root.shape, &mesh, kernel, tolerance, cache)?;
        shapes.push(EvaluatedShape {
            mesh,
            source_range: root.source_range,
            parameters: root.parameters,
            parts,
            tree,
        });
    }

    Ok(Evaluation { shapes })
}

/// Convenience wrapper for callers that only need final meshes.
pub fn evaluate_to_meshes<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Vec<Mesh>, EvalError> {
    Ok(evaluate(source, kernel, tolerance)?
        .shapes
        .into_iter()
        .map(|shape| shape.mesh)
        .collect())
}

#[derive(Debug)]
struct Node {
    shape: ShapeId,
    module: String,
    source_range: Range<usize>,
    parameters: Vec<Parameter>,
    transform_parameters: Option<Vec<Parameter>>,
    world: Transform,
    children: Vec<Node>,
}

struct Evaluator<'a, K: Kernel> {
    kernel: &'a mut K,
    variables: HashMap<String, Value>,
    assignments: HashMap<String, SyntaxNode>,
    tolerance: f64,
}

impl<K: Kernel> Evaluator<'_, K> {
    fn statement(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Option<Node>, EvalError> {
        match node.kind() {
            SyntaxKind::ASSIGNMENT => {
                self.assignment(node)?;
                Ok(None)
            }
            SyntaxKind::MODULE_CALL => self.module_call(node, parent).map(Some),
            _ => Ok(None),
        }
    }

    fn assignment(&mut self, node: SyntaxNode) -> Result<(), EvalError> {
        let name = first_ident(&node)
            .ok_or_else(|| EvalError::Parse("missing assignment name".to_owned()))?;
        let value = self
            .expression_after_eq(&node)?
            .ok_or_else(|| EvalError::Parse(format!("missing expression for {name}")))?;
        self.assignments.insert(name.clone(), node);
        self.variables.insert(name, value);
        Ok(())
    }

    fn module_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Node, EvalError> {
        let name =
            first_ident(&node).ok_or_else(|| EvalError::Parse("missing module name".to_owned()))?;
        let module = name.clone();
        let source_range = text_range_to_range(node.text_range());
        let parameters = collect_parameters(&node, &self.assignments);

        let args = match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        let block = node
            .children()
            .find(|child| child.kind() == SyntaxKind::BLOCK);

        match name.as_str() {
            "cube" => {
                args.ensure_known("cube", &["size"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("cube", "expected one argument"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("cube", "expected size"))?
                    .as_size("cube")?;
                let shape = self.kernel.primitive(Primitive::Cuboid { size })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    children: Vec::new(),
                })
            }
            "cylinder" => {
                args.ensure_known("cylinder", &["r", "h"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("cylinder", "expected radius and height"));
                }
                let radius = args
                    .argument("r", 0)
                    .ok_or_else(|| bad_args("cylinder", "expected radius and height"))?
                    .as_number("cylinder")?;
                let height = args
                    .argument("h", 1)
                    .ok_or_else(|| bad_args("cylinder", "expected radius and height"))?
                    .as_number("cylinder")?;
                let shape = self
                    .kernel
                    .primitive(Primitive::Cylinder { radius, height })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    children: Vec::new(),
                })
            }
            "sphere" => {
                args.ensure_known("sphere", &["r", "d"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("sphere", "expected one radius"));
                }
                let radius = if let Some(value) = args.named.get("r") {
                    value.as_number("sphere")?
                } else if let Some(value) = args.named.get("d") {
                    value.as_number("sphere")? / 2.0
                } else {
                    args.argument("r", 0)
                        .ok_or_else(|| bad_args("sphere", "expected radius"))?
                        .as_number("sphere")?
                };
                let shape = self.kernel.primitive(Primitive::Sphere { radius })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    children: Vec::new(),
                })
            }
            "resize" => {
                args.ensure_known("resize", &["newsize"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("resize", "expected one argument"));
                }
                let newsize = args
                    .argument("newsize", 0)
                    .ok_or_else(|| bad_args("resize", "expected newsize"))?
                    .as_vector3("resize")?;
                if newsize.iter().any(|value| *value < 0.0) {
                    return Err(bad_args("resize", "dimensions must be non-negative"));
                }

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignments))
                    .unwrap_or_default();

                let mut children = match block {
                    Some(block) => self.block(block, Transform::IDENTITY)?,
                    None => {
                        let child = node
                            .children()
                            .find(|child| child.kind() == SyntaxKind::MODULE_CALL)
                            .ok_or_else(|| bad_args("resize", "expected a child or block"))?;
                        vec![self.module_call(child, Transform::IDENTITY)?]
                    }
                };
                if children.is_empty() {
                    return Err(bad_args("resize", "expected at least one child"));
                }

                let mut united = children[0].shape;
                for child in &children[1..] {
                    united = self.kernel.boolean(BooleanOp::Union, united, child.shape)?;
                }
                let mesh = self.kernel.tessellate(united, self.tolerance)?;
                let (min, max) = mesh_bounds(&mesh)
                    .ok_or_else(|| bad_args("resize", "cannot resize empty geometry"))?;

                let mut scale = [1.0; 3];
                for axis in 0..3 {
                    if newsize[axis] == 0.0 {
                        continue;
                    }
                    let extent = max[axis] - min[axis];
                    if extent <= f64::EPSILON {
                        return Err(bad_args("resize", "cannot resize a degenerate dimension"));
                    }
                    scale[axis] = newsize[axis] / extent;
                }

                let combined = parent.compose(Transform::from_scale(scale));
                check_transform(combined)?;
                for child in &mut children {
                    self.transform_tree(child, combined)?;
                }

                let mut result = children[0].shape;
                for child in &children[1..] {
                    result = self.kernel.boolean(BooleanOp::Union, result, child.shape)?;
                }

                Ok(Node {
                    shape: result,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: combined,
                    children,
                })
            }
            "translate" | "rotate" | "scale" | "multmatrix" | "mirror" => {
                let first = match name.as_str() {
                    "rotate" => "a",
                    "multmatrix" => "m",
                    _ => "v",
                };
                args.ensure_known(name.as_str(), &[first])?;
                let argument = args.one(name.as_str(), first)?;
                let local = match name.as_str() {
                    "translate" => Transform::from_translation(argument.as_vector3("translate")?),
                    "rotate" => {
                        let [x, y, z] = argument.as_vector3("rotate")?;
                        Transform::from_rotation_xyz([
                            x.to_radians(),
                            y.to_radians(),
                            z.to_radians(),
                        ])
                    }
                    "multmatrix" => argument.as_matrix("multmatrix")?,
                    "mirror" => {
                        let normal = argument.as_vector3("mirror")?;
                        Transform::from_mirror(normal).ok_or_else(|| {
                            bad_args("mirror", "normal must be non-zero and finite")
                        })?
                    }
                    _ => Transform::from_scale(argument.as_scale("scale")?),
                };
                let combined = parent.compose(local);

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignments))
                    .unwrap_or_default();

                let children = match block {
                    Some(block) => self.block(block, combined)?,
                    None => {
                        let child = node
                            .children()
                            .find(|child| child.kind() == SyntaxKind::MODULE_CALL)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a child or block"))?;
                        vec![self.module_call(child, combined)?]
                    }
                };
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }

                let mut shapes = children.iter().map(|child| child.shape);
                let mut result = shapes.next().expect("checked non-empty");
                for shape in shapes {
                    result = self.kernel.boolean(BooleanOp::Union, result, shape)?;
                }

                Ok(Node {
                    shape: result,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: combined,
                    children,
                })
            }
            "difference" | "union" | "intersection" => {
                args.ensure_empty(name.as_str())?;
                let children = match block {
                    Some(block) => self.block(block, parent)?,
                    None => {
                        let child = node
                            .children()
                            .find(|child| child.kind() == SyntaxKind::MODULE_CALL)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a block or child"))?;
                        vec![self.module_call(child, parent)?]
                    }
                };
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }

                let op = match name.as_str() {
                    "difference" => BooleanOp::Difference,
                    "union" => BooleanOp::Union,
                    _ => BooleanOp::Intersection,
                };
                let mut shapes = children.iter().map(|child| child.shape);
                let mut result = shapes.next().expect("checked non-empty");
                for shape in shapes {
                    result = self.kernel.boolean(op, result, shape)?;
                }

                Ok(Node {
                    shape: result,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    children,
                })
            }
            _ => Err(EvalError::UnsupportedModule(name)),
        }
    }

    fn block(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let mut nodes = Vec::new();
        for statement in node.children() {
            if let Some(node) = self.statement(statement, parent)? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn apply_transform(
        &mut self,
        shape: ShapeId,
        transform: Transform,
    ) -> Result<ShapeId, EvalError> {
        check_transform(transform)?;
        if transform.is_identity() {
            Ok(shape)
        } else {
            Ok(self.kernel.transform(shape, transform)?)
        }
    }

    fn transform_tree(&mut self, node: &mut Node, transform: Transform) -> Result<(), EvalError> {
        if transform.is_identity() {
            return Ok(());
        }
        node.shape = self.kernel.transform(node.shape, transform)?;
        node.world = transform.compose(node.world);
        for child in &mut node.children {
            self.transform_tree(child, transform)?;
        }
        Ok(())
    }

    fn arg_list(&mut self, node: SyntaxNode) -> Result<Arguments, EvalError> {
        let mut arguments = Arguments::default();
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::COMMA => {}
                kind if kind.is_trivia() => {}
                SyntaxKind::NAMED_ARG => {
                    let Some(child) = element.into_node() else {
                        continue;
                    };
                    let name = first_ident(&child)
                        .ok_or_else(|| EvalError::Parse("missing argument name".to_owned()))?;
                    let value = self.expression_after_eq(&child)?.ok_or_else(|| {
                        EvalError::Parse(format!("missing value for argument {name}"))
                    })?;
                    arguments.named.insert(name, value);
                }
                _ => arguments.positional.push(self.element_value(element)?),
            }
        }
        Ok(arguments)
    }

    fn element_value(&mut self, element: SyntaxElement) -> Result<Value, EvalError> {
        match element {
            NodeOrToken::Token(token) => match token.kind() {
                SyntaxKind::NUMBER => Ok(Value::Number(parse_number(token.text())?)),
                SyntaxKind::IDENT => self.lookup(token.text()),
                kind => Err(EvalError::Expression(format!(
                    "unexpected token in expression: {kind:?}"
                ))),
            },
            NodeOrToken::Node(node) => self.node_value(&node),
        }
    }

    fn node_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        match node.kind() {
            SyntaxKind::VECTOR => self.vector(node.clone()),
            SyntaxKind::BINARY_EXPR => self.binary_expr_value(node),
            SyntaxKind::UNARY_EXPR => self.unary_expr_value(node),
            SyntaxKind::GROUP => self.group_value(node),
            kind => Err(EvalError::Expression(format!(
                "unexpected node in expression: {kind:?}"
            ))),
        }
    }

    fn binary_expr_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut operands = Vec::new();
        let mut op = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::PERCENT => op = Some(element.kind()),
                kind if kind.is_trivia() => {}
                _ => operands.push(self.element_value(element)?),
            }
        }

        let mut operands = operands.into_iter();
        let left = operands
            .next()
            .ok_or_else(|| EvalError::Expression("missing left operand".to_owned()))?;
        let right = operands
            .next()
            .ok_or_else(|| EvalError::Expression("missing right operand".to_owned()))?;
        if operands.next().is_some() {
            return Err(EvalError::Expression(
                "too many operands in binary expression".to_owned(),
            ));
        }
        let op = op.ok_or_else(|| EvalError::Expression("missing operator".to_owned()))?;
        binary_value(op, left, right)
    }

    fn unary_expr_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut operand = None;
        let mut op = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::PLUS | SyntaxKind::MINUS => op = Some(element.kind()),
                kind if kind.is_trivia() => {}
                _ => operand = Some(self.element_value(element)?),
            }
        }
        unary_value(
            op.ok_or_else(|| EvalError::Expression("missing operator".to_owned()))?,
            operand.ok_or_else(|| EvalError::Expression("missing operand".to_owned()))?,
        )
    }

    fn group_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
                kind if kind.is_trivia() => {}
                _ => return self.element_value(element),
            }
        }
        Err(EvalError::Expression("empty group".to_owned()))
    }

    fn expression_after_eq(&mut self, node: &SyntaxNode) -> Result<Option<Value>, EvalError> {
        let mut after_eq = false;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::EQ => after_eq = true,
                kind if kind.is_trivia() => {}
                _ if after_eq => return Ok(Some(self.element_value(element)?)),
                _ => {}
            }
        }
        Ok(None)
    }

    fn vector(&mut self, node: SyntaxNode) -> Result<Value, EvalError> {
        let mut numbers = Vec::new();
        let mut rows = Vec::new();
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_BRACK | SyntaxKind::R_BRACK | SyntaxKind::COMMA => {}
                kind if kind.is_trivia() => {}
                _ => match self.element_value(element)? {
                    Value::Number(value) => numbers.push(value),
                    Value::Vector(values) => rows.push(values),
                    Value::Matrix(_) => {
                        return Err(EvalError::Expression(
                            "vectors support at most two levels".to_owned(),
                        ));
                    }
                },
            }
        }

        if rows.is_empty() {
            Ok(Value::Vector(numbers))
        } else if numbers.is_empty() {
            Ok(Value::Matrix(rows))
        } else {
            Err(EvalError::Expression(
                "cannot mix scalars and vectors in one vector".to_owned(),
            ))
        }
    }

    fn lookup(&self, name: &str) -> Result<Value, EvalError> {
        self.variables
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownIdentifier(name.to_owned()))
    }
}

struct PartContext<'a, K: Kernel> {
    kernel: &'a mut K,
    root_shape: ShapeId,
    root_mesh: &'a Mesh,
    tolerance: f64,
    cache: &'a mut MeshCache,
}

fn collect_parts<K: Kernel>(
    node: &Node,
    root_shape: ShapeId,
    root_mesh: &Mesh,
    kernel: &mut K,
    tolerance: f64,
    cache: &mut MeshCache,
) -> Result<(Vec<Part>, TreeNode), EvalError> {
    let mut parts = Vec::new();
    let mut context = PartContext {
        kernel,
        root_shape,
        root_mesh,
        tolerance,
        cache,
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

    if node.children.is_empty() {
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
        parts.push(Part {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            parameters,
            mesh,
        });
        Ok(TreeNode {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            children: Vec::new(),
            part_index: Some(part_index),
        })
    } else {
        let mut children = Vec::new();
        for child in &node.children {
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

fn check_transform(transform: Transform) -> Result<(), EvalError> {
    if !transform.is_finite() {
        return Err(EvalError::InvalidTransform(
            "matrix contains non-finite values".to_owned(),
        ));
    }
    if transform.determinant().abs() < SINGULAR_DETERMINANT {
        return Err(EvalError::InvalidTransform("matrix is singular".to_owned()));
    }
    Ok(())
}

fn mesh_bounds(mesh: &Mesh) -> Option<([f64; 3], [f64; 3])> {
    if mesh.positions.is_empty() {
        return None;
    }

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    Some((min, max))
}
