//! Evaluator connecting the source tree to a kernel.

use std::collections::HashMap;

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Sketch, Transform};
use brepscad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, parse};

use crate::cache::MeshCache;
use crate::model::{EvaluatedShape, Evaluation};
use crate::parameters::{collect_parameters, first_ident, parse_number, text_range_to_range};
use crate::parts::{Dimension, Node, collect_parts};
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

    fn radius(&self, module: &str) -> Result<f64, EvalError> {
        if self.positional.len() > 1 {
            return Err(bad_args(module, "expected one radius"));
        }
        if let Some(value) = self.named.get("r") {
            value.as_number(module)
        } else if let Some(value) = self.named.get("d") {
            Ok(value.as_number(module)? / 2.0)
        } else {
            self.argument("r", 0)
                .ok_or_else(|| bad_args(module, "expected radius"))?
                .as_number(module)
        }
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
                    dimension: Dimension::ThreeD,
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
                    dimension: Dimension::ThreeD,
                    children: Vec::new(),
                })
            }
            "sphere" => {
                args.ensure_known("sphere", &["r", "d"])?;
                let radius = args.radius("sphere")?;
                let shape = self.kernel.primitive(Primitive::Sphere { radius })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::ThreeD,
                    children: Vec::new(),
                })
            }
            "square" => {
                args.ensure_known("square", &["size"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("square", "expected one size"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("square", "expected size"))?
                    .as_size2("square")?;
                let shape = self.kernel.sketch(Sketch::Square { size })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::TwoD,
                    children: Vec::new(),
                })
            }
            "circle" => {
                args.ensure_known("circle", &["r", "d"])?;
                let radius = args.radius("circle")?;
                let shape = self.kernel.sketch(Sketch::Circle { radius })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::TwoD,
                    children: Vec::new(),
                })
            }
            "polygon" => {
                args.ensure_known("polygon", &["points"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("polygon", "expected one point list"));
                }
                let points = args
                    .argument("points", 0)
                    .ok_or_else(|| bad_args("polygon", "expected points"))?
                    .as_polygon_points("polygon")?;
                let shape = self.kernel.sketch(Sketch::Polygon { points })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::TwoD,
                    children: Vec::new(),
                })
            }
            "linear_extrude" => {
                args.ensure_known("linear_extrude", &["height"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("linear_extrude", "expected one height"));
                }
                let height = args
                    .argument("height", 0)
                    .ok_or_else(|| bad_args("linear_extrude", "expected height"))?
                    .as_number("linear_extrude")?;

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignments))
                    .unwrap_or_default();

                let children = match block {
                    Some(block) => self.block(block, Transform::IDENTITY)?,
                    None => {
                        let child = node
                            .children()
                            .find(|child| child.kind() == SyntaxKind::MODULE_CALL)
                            .ok_or_else(|| {
                                bad_args("linear_extrude", "expected a child or block")
                            })?;
                        vec![self.module_call(child, Transform::IDENTITY)?]
                    }
                };
                if children.len() != 1 {
                    return Err(bad_args("linear_extrude", "expected exactly one 2D child"));
                }
                if children[0].dimension != Dimension::TwoD {
                    return Err(bad_args("linear_extrude", "expected a 2D child"));
                }

                let shape = self.kernel.extrude(children[0].shape, height)?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: parent,
                    dimension: Dimension::ThreeD,
                    children,
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
                let dimension = children[0].dimension;
                if children.iter().any(|child| child.dimension != dimension) {
                    return Err(bad_args("resize", "cannot mix 2D and 3D children"));
                }
                if dimension == Dimension::TwoD && children.len() > 1 {
                    return Err(bad_args("resize", "2D unions are not supported"));
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
                    dimension,
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
                let dimension = children[0].dimension;
                if children.iter().any(|child| child.dimension != dimension) {
                    return Err(bad_args(name.as_str(), "cannot mix 2D and 3D children"));
                }
                if dimension == Dimension::TwoD && children.len() > 1 {
                    return Err(bad_args(name.as_str(), "2D unions are not supported"));
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
                    dimension,
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
                if children
                    .iter()
                    .any(|child| child.dimension != Dimension::ThreeD)
                {
                    return Err(bad_args(name.as_str(), "2D booleans are not supported"));
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
                    dimension: Dimension::ThreeD,
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
