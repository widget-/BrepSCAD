//! Minimal evaluator connecting the lossless SCAD syntax tree to a kernel.
//!
//! The supported subset is deliberately small: top-level assignments,
//! `cube`, `cylinder`, and the boolean modules `difference`, `union`, and
//! `intersection` with brace-delimited children.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use thiserror::Error;
use visual_openscad_kernel_api::{
    BooleanOp, Kernel, KernelError, Mesh, Primitive, ShapeId, Transform,
};
use visual_openscad_scad_syntax::{SyntaxKind, SyntaxNode, TextRange, parse};

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unknown identifier: {0}")]
    UnknownIdentifier(String),
    #[error("unsupported module: {0}")]
    UnsupportedModule(String),
    #[error("bad arguments for {module}: {message}")]
    BadArguments { module: String, message: String },
    #[error("kernel error: {0}")]
    Kernel(#[from] KernelError),
}

/// The result of evaluating a source file.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub shapes: Vec<EvaluatedShape>,
}

/// One top-level shape, its final mesh, and its selectable operands.
#[derive(Debug, Clone)]
pub struct EvaluatedShape {
    pub mesh: Mesh,
    pub source_range: Range<usize>,
    pub parameters: Vec<Parameter>,
    pub parts: Vec<Part>,
}

/// A selectable operand, usually a primitive inside a boolean.
#[derive(Debug, Clone)]
pub struct Part {
    pub module: String,
    pub source_range: Range<usize>,
    pub parameters: Vec<Parameter>,
    pub mesh: Mesh,
}

/// A numeric literal reachable from a shape or part's source range.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub range: Range<usize>,
    pub value: f64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Number(f64),
    Vector(Vec<f64>),
}

impl Value {
    fn as_number(&self, module: &str) -> Result<f64, EvalError> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Vector(_) => Err(bad_args(module, "expected a number")),
        }
    }

    fn as_size(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 3]),
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            Self::Vector(_) => Err(bad_args(module, "expected a 3-element vector or a scalar")),
        }
    }

    fn as_vector3(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            _ => Err(bad_args(module, "expected a 3-element vector")),
        }
    }

    fn as_scale(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 3]),
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
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
        };

        for statement in parsed.syntax().children() {
            if let Some(node) = evaluator.statement(statement)? {
                roots.push(node);
            }
        }
    }

    let mut shapes = Vec::new();
    for root in roots {
        let mesh = kernel.tessellate(root.shape, tolerance)?;
        let parts = collect_parts(&root, root.shape, &mesh, kernel, tolerance)?;
        shapes.push(EvaluatedShape {
            mesh,
            source_range: root.source_range,
            parameters: root.parameters,
            parts,
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
    children: Vec<Node>,
}

struct Evaluator<'a, K: Kernel> {
    kernel: &'a mut K,
    variables: HashMap<String, Value>,
    assignments: HashMap<String, SyntaxNode>,
}

impl<K: Kernel> Evaluator<'_, K> {
    fn statement(&mut self, node: SyntaxNode) -> Result<Option<Node>, EvalError> {
        match node.kind() {
            SyntaxKind::ASSIGNMENT => {
                self.assignment(node)?;
                Ok(None)
            }
            SyntaxKind::MODULE_CALL => self.module_call(node).map(Some),
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

    fn module_call(&mut self, node: SyntaxNode) -> Result<Node, EvalError> {
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
            None => Vec::new(),
        };
        let block = node
            .children()
            .find(|child| child.kind() == SyntaxKind::BLOCK);

        match name.as_str() {
            "cube" => {
                let size = single_arg(&args, "cube")?.as_size("cube")?;
                let shape = self.kernel.primitive(Primitive::Cuboid { size })?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    children: Vec::new(),
                })
            }
            "cylinder" => {
                if args.len() != 2 {
                    return Err(bad_args("cylinder", "expected radius and height"));
                }
                let radius = args[0].as_number("cylinder")?;
                let height = args[1].as_number("cylinder")?;
                let shape = self
                    .kernel
                    .primitive(Primitive::Cylinder { radius, height })?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    children: Vec::new(),
                })
            }
            "translate" | "rotate" | "scale" => {
                let argument = single_arg(&args, name.as_str())?;
                let transform = match name.as_str() {
                    "translate" => Transform::Translate(argument.as_vector3("translate")?),
                    "rotate" => {
                        let [x, y, z] = argument.as_vector3("rotate")?;
                        Transform::Rotate([x.to_radians(), y.to_radians(), z.to_radians()])
                    }
                    _ => Transform::Scale(argument.as_scale("scale")?),
                };

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignments))
                    .unwrap_or_default();

                let mut children = match block {
                    Some(block) => self.block(block)?,
                    None => {
                        let child = node
                            .children()
                            .find(|child| child.kind() == SyntaxKind::MODULE_CALL)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a child or block"))?;
                        vec![self.module_call(child)?]
                    }
                };
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }

                for child in &mut children {
                    self.transform_node(child, transform)?;
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
                    children,
                })
            }
            "difference" | "union" | "intersection" => {
                let block = block.ok_or_else(|| bad_args(name.as_str(), "expected a block"))?;
                let children = self.block(block)?;
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
                    children,
                })
            }
            _ => Err(EvalError::UnsupportedModule(name)),
        }
    }

    fn block(&mut self, node: SyntaxNode) -> Result<Vec<Node>, EvalError> {
        let mut nodes = Vec::new();
        for statement in node.children() {
            if let Some(node) = self.statement(statement)? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    fn transform_node(&mut self, node: &mut Node, transform: Transform) -> Result<(), EvalError> {
        node.shape = self.kernel.transform(node.shape, transform)?;
        for child in &mut node.children {
            self.transform_node(child, transform)?;
        }
        Ok(())
    }

    fn arg_list(&mut self, node: SyntaxNode) -> Result<Vec<Value>, EvalError> {
        let mut args = Vec::new();
        for element in node.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::NUMBER => args.push(Value::Number(parse_number(token.text())?)),
                    SyntaxKind::IDENT => args.push(self.lookup(token.text())?),
                    _ => {}
                }
            } else if let Some(child) = element.as_node() {
                if child.kind() == SyntaxKind::VECTOR {
                    args.push(self.vector(child.clone())?);
                }
            }
        }
        Ok(args)
    }

    fn expression_after_eq(&mut self, node: &SyntaxNode) -> Result<Option<Value>, EvalError> {
        let mut after_eq = false;
        for element in node.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::EQ {
                    after_eq = true;
                    continue;
                }
                if !after_eq || token.kind().is_trivia() {
                    continue;
                }
                match token.kind() {
                    SyntaxKind::NUMBER => {
                        return Ok(Some(Value::Number(parse_number(token.text())?)));
                    }
                    SyntaxKind::IDENT => return Ok(Some(self.lookup(token.text())?)),
                    _ => {}
                }
            } else if let Some(child) = element.as_node() {
                if after_eq && child.kind() == SyntaxKind::VECTOR {
                    return Ok(Some(self.vector(child.clone())?));
                }
            }
        }
        Ok(None)
    }

    fn vector(&mut self, node: SyntaxNode) -> Result<Value, EvalError> {
        let mut values = Vec::new();
        for element in node.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::NUMBER => values.push(parse_number(token.text())?),
                    SyntaxKind::IDENT => {
                        values.push(self.lookup(token.text())?.as_number("vector")?);
                    }
                    _ => {}
                }
            } else if let Some(child) = element.as_node() {
                if child.kind() == SyntaxKind::VECTOR {
                    return Err(EvalError::Parse(
                        "nested vectors are not supported".to_owned(),
                    ));
                }
            }
        }
        Ok(Value::Vector(values))
    }

    fn lookup(&self, name: &str) -> Result<Value, EvalError> {
        self.variables
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownIdentifier(name.to_owned()))
    }
}

fn collect_parts<K: Kernel>(
    node: &Node,
    root_shape: ShapeId,
    root_mesh: &Mesh,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Vec<Part>, EvalError> {
    let mut parts = Vec::new();
    collect_parts_inner(
        node,
        root_shape,
        root_mesh,
        kernel,
        tolerance,
        &BTreeMap::new(),
        &mut parts,
    )?;
    Ok(parts)
}

fn collect_parts_inner<K: Kernel>(
    node: &Node,
    root_shape: ShapeId,
    root_mesh: &Mesh,
    kernel: &mut K,
    tolerance: f64,
    inherited: &BTreeMap<usize, Parameter>,
    parts: &mut Vec<Part>,
) -> Result<(), EvalError> {
    let mut scope = inherited.clone();
    if let Some(transform_parameters) = &node.transform_parameters {
        for parameter in transform_parameters {
            scope
                .entry(parameter.range.start)
                .or_insert_with(|| parameter.clone());
        }
    }

    if node.children.is_empty() {
        let mesh = if node.shape == root_shape {
            root_mesh.clone()
        } else {
            kernel.tessellate(node.shape, tolerance)?
        };
        for parameter in &node.parameters {
            scope
                .entry(parameter.range.start)
                .or_insert_with(|| parameter.clone());
        }
        parts.push(Part {
            module: node.module.clone(),
            source_range: node.source_range.clone(),
            parameters: scope.into_values().collect(),
            mesh,
        });
    } else {
        for child in &node.children {
            collect_parts_inner(
                child, root_shape, root_mesh, kernel, tolerance, &scope, parts,
            )?;
        }
    }
    Ok(())
}

fn first_ident(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_owned())
}

fn parse_number(text: &str) -> Result<f64, EvalError> {
    text.parse::<f64>()
        .map_err(|_| EvalError::Parse(format!("invalid number: {text}")))
}

fn single_arg<'a>(args: &'a [Value], module: &str) -> Result<&'a Value, EvalError> {
    if args.len() == 1 {
        Ok(&args[0])
    } else {
        Err(bad_args(module, "expected one argument"))
    }
}

fn bad_args(module: &str, message: impl Into<String>) -> EvalError {
    EvalError::BadArguments {
        module: module.to_owned(),
        message: message.into(),
    }
}

fn text_range_to_range(range: TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}

fn collect_parameters(
    node: &SyntaxNode,
    assignments: &HashMap<String, SyntaxNode>,
) -> Vec<Parameter> {
    let mut parameters = BTreeMap::new();
    collect_numbers(node, &mut parameters);

    let mut names = BTreeSet::new();
    collect_identifiers(node, &mut names);

    let mut visited = BTreeSet::new();
    for name in names {
        collect_assignment_numbers(&name, assignments, &mut parameters, &mut visited);
    }

    parameters.into_values().collect()
}

fn collect_numbers(node: &SyntaxNode, parameters: &mut BTreeMap<usize, Parameter>) {
    for element in node.descendants_with_tokens() {
        let Some(token) = element.as_token() else {
            continue;
        };
        if token.kind() != SyntaxKind::NUMBER {
            continue;
        }
        let Ok(value) = token.text().parse::<f64>() else {
            continue;
        };
        let range = text_range_to_range(token.text_range());
        parameters.entry(range.start).or_insert_with(|| Parameter {
            range,
            value,
            source: token.text().to_owned(),
        });
    }
}

fn collect_identifiers(node: &SyntaxNode, names: &mut BTreeSet<String>) {
    for element in node.descendants_with_tokens() {
        let Some(token) = element.as_token() else {
            continue;
        };
        if token.kind() == SyntaxKind::IDENT {
            names.insert(token.text().to_owned());
        }
    }
}

fn collect_assignment_numbers(
    name: &str,
    assignments: &HashMap<String, SyntaxNode>,
    parameters: &mut BTreeMap<usize, Parameter>,
    visited: &mut BTreeSet<String>,
) {
    if !visited.insert(name.to_owned()) {
        return;
    }
    let Some(node) = assignments.get(name) else {
        return;
    };

    collect_numbers(node, parameters);

    let mut names = BTreeSet::new();
    collect_identifiers(node, &mut names);
    for nested in names {
        collect_assignment_numbers(&nested, assignments, parameters, visited);
    }
}
