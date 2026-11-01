//! Evaluator connecting the source tree to a kernel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Sketch, Transform};
use brepscad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, parse};

use crate::cache::MeshCache;
use crate::extrude;
use crate::hull;
use crate::model::{EvaluatedShape, Evaluation, Parameter};
use crate::parameters::{collect_parameters, first_ident, parse_number, text_range_to_range};
use crate::parts::{Dimension, Node, collect_parts};
use crate::profile;
use crate::scope::{Declaration, Env};
use crate::value::{FunctionValue, Value, binary_value, unary_value};
use crate::{EvalError, bad_args};

const SINGULAR_DETERMINANT: f64 = 1.0e-12;
const FACETED_MAX: usize = 32;
const PROFILE_SEGMENTS_MAX: usize = 256;
const MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Default)]
struct Arguments {
    positional: Vec<Value>,
    named: HashMap<String, Value>,
    named_order: Vec<String>,
    special: HashMap<String, Value>,
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

/// Evaluation settings shared by the CLI, viewer, and tests.
#[derive(Debug, Clone)]
pub struct EvaluationOptions {
    pub tolerance: f64,
    /// Place cubes/squares at the origin, cylinder bases at `z = 0`, and
    /// linear extrusions from `z = 0`, matching OpenSCAD defaults.
    pub legacy_centering: bool,
    /// Directories searched by angle-bracket `include` / `use` paths.
    pub library_paths: Vec<PathBuf>,
}

impl Default for EvaluationOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.01,
            legacy_centering: false,
            library_paths: Vec::new(),
        }
    }
}

/// Reads `OPENSCADPATH` using the platform path separator.
pub fn environment_library_paths() -> Vec<PathBuf> {
    std::env::var_os("OPENSCADPATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default()
}

/// Parses and evaluates `source`, returning each top-level shape with its
/// source range, numeric parameter bindings, and selectable operands.
pub fn evaluate<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Evaluation, EvalError> {
    evaluate_with_options(
        source,
        kernel,
        EvaluationOptions {
            tolerance,
            legacy_centering: false,
            library_paths: Vec::new(),
        },
    )
}

/// Like [`evaluate`], with explicit evaluation options.
pub fn evaluate_with_options<K: Kernel>(
    source: &str,
    kernel: &mut K,
    options: EvaluationOptions,
) -> Result<Evaluation, EvalError> {
    evaluate_with_cache_options(source, kernel, options, &mut MeshCache::default())
}

/// Like [`evaluate`], but reuses leaf meshes from `cache`.
pub fn evaluate_with_cache<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
    cache: &mut MeshCache,
) -> Result<Evaluation, EvalError> {
    evaluate_with_cache_options(
        source,
        kernel,
        EvaluationOptions {
            tolerance,
            legacy_centering: false,
            library_paths: Vec::new(),
        },
        cache,
    )
}

/// Like [`evaluate_with_options`], but reuses leaf meshes from `cache`.
pub fn evaluate_with_cache_options<K: Kernel>(
    source: &str,
    kernel: &mut K,
    options: EvaluationOptions,
    cache: &mut MeshCache,
) -> Result<Evaluation, EvalError> {
    evaluate_with_context(source, None, kernel, options, cache)
}

/// Parses and evaluates a source file, resolving `include` and `use` paths
/// relative to that file.
pub fn evaluate_file<K: Kernel>(
    path: impl AsRef<Path>,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Evaluation, EvalError> {
    evaluate_file_with_options(
        path,
        kernel,
        EvaluationOptions {
            tolerance,
            legacy_centering: false,
            library_paths: Vec::new(),
        },
    )
}

/// Like [`evaluate_file`], with explicit evaluation options.
pub fn evaluate_file_with_options<K: Kernel>(
    path: impl AsRef<Path>,
    kernel: &mut K,
    options: EvaluationOptions,
) -> Result<Evaluation, EvalError> {
    evaluate_file_with_cache_options(path, kernel, options, &mut MeshCache::default())
}

/// Like [`evaluate_file_with_options`], but reuses leaf meshes from `cache`.
pub fn evaluate_file_with_cache_options<K: Kernel>(
    path: impl AsRef<Path>,
    kernel: &mut K,
    options: EvaluationOptions,
    cache: &mut MeshCache,
) -> Result<Evaluation, EvalError> {
    let path = path.as_ref();
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let source = std::fs::read_to_string(&canonical).map_err(|error| {
        EvalError::Source(format!("failed to read {}: {error}", canonical.display()))
    })?;
    evaluate_with_context(&source, Some(canonical), kernel, options, cache)
}

/// Evaluates in-memory `source` with a file path supplying include/use context.
pub fn evaluate_source_with_path<K: Kernel>(
    source: &str,
    source_path: impl AsRef<Path>,
    kernel: &mut K,
    options: EvaluationOptions,
    cache: &mut MeshCache,
) -> Result<Evaluation, EvalError> {
    let path = source_path.as_ref();
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    evaluate_with_context(source, Some(canonical), kernel, options, cache)
}

fn evaluate_with_context<K: Kernel>(
    source: &str,
    source_path: Option<PathBuf>,
    kernel: &mut K,
    options: EvaluationOptions,
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
        let messages = match &source_path {
            Some(path) => format!("{}: {messages}", path.display()),
            None => messages,
        };
        return Err(EvalError::Parse(messages));
    }

    let tolerance = options.tolerance;
    let (source_dir, include_stack) = match source_path {
        Some(path) => (path.parent().map(Path::to_path_buf), vec![path]),
        None => (None, Vec::new()),
    };
    let mut evaluator = Evaluator {
        kernel,
        scopes: vec![{
            let root = Env::root();
            let mut variables = root.variables.borrow_mut();
            variables.insert("$fa".to_owned(), Value::Number(12.0));
            variables.insert("$fs".to_owned(), Value::Number(2.0));
            variables.insert("$fn".to_owned(), Value::Number(0.0));
            variables.insert("$t".to_owned(), Value::Number(0.0));
            variables.insert("$preview".to_owned(), Value::Boolean(false));
            variables.insert("PI".to_owned(), Value::Number(std::f64::consts::PI));
            drop(variables);
            root
        }],
        children_stack: Vec::new(),
        tolerance,
        legacy_centering: options.legacy_centering,
        library_paths: options.library_paths,
        source_dir,
        include_stack,
        messages: Vec::new(),
        call_depth: 0,
        call_stack: Vec::new(),
    };
    let roots = evaluator.evaluate_source_tree(&parsed.syntax(), Transform::IDENTITY)?;
    let messages = std::mem::take(&mut evaluator.messages);
    drop(evaluator);

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

    Ok(Evaluation { shapes, messages })
}

/// Convenience wrapper for callers that only need final meshes.
pub fn evaluate_to_meshes<K: Kernel>(
    source: &str,
    kernel: &mut K,
    tolerance: f64,
) -> Result<Vec<Mesh>, EvalError> {
    evaluate_to_meshes_with_options(
        source,
        kernel,
        EvaluationOptions {
            tolerance,
            legacy_centering: false,
            library_paths: Vec::new(),
        },
    )
}

/// Like [`evaluate_to_meshes`], with explicit evaluation options.
pub fn evaluate_to_meshes_with_options<K: Kernel>(
    source: &str,
    kernel: &mut K,
    options: EvaluationOptions,
) -> Result<Vec<Mesh>, EvalError> {
    Ok(evaluate_with_options(source, kernel, options)?
        .shapes
        .into_iter()
        .map(|shape| shape.mesh)
        .collect())
}

struct Evaluator<'a, K: Kernel> {
    kernel: &'a mut K,
    scopes: Vec<Rc<Env>>,
    children_stack: Vec<ChildrenFrame>,
    tolerance: f64,
    legacy_centering: bool,
    library_paths: Vec<PathBuf>,
    source_dir: Option<PathBuf>,
    include_stack: Vec<PathBuf>,
    messages: Vec<String>,
    call_depth: usize,
    call_stack: Vec<String>,
}

#[derive(Debug)]
struct ChildrenFrame {
    statements: Vec<SyntaxNode>,
    caller_env: Rc<Env>,
    special: HashMap<String, Value>,
}

impl<K: Kernel> Evaluator<'_, K> {
    fn statement(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        match node.kind() {
            SyntaxKind::ASSIGNMENT => {
                self.assignment(node)?;
                Ok(Vec::new())
            }
            SyntaxKind::MODULE_DECLARATION => Ok(Vec::new()),
            SyntaxKind::FUNCTION_DECLARATION => Ok(Vec::new()),
            SyntaxKind::INCLUDE => self.include_file(node, parent, false),
            SyntaxKind::USE => self.include_file(node, parent, true),
            SyntaxKind::MODULE_CALL => self.module_call(node, parent),
            SyntaxKind::MODIFIER => self.modified_statement(node, parent),
            SyntaxKind::BLOCK => self.block(node, parent),
            _ => Ok(Vec::new()),
        }
    }

    fn assignment(&mut self, node: SyntaxNode) -> Result<(), EvalError> {
        let name = first_ident(&node)
            .ok_or_else(|| EvalError::Parse("missing assignment name".to_owned()))?;
        let value = self
            .expression_after_eq(&node)?
            .ok_or_else(|| EvalError::Parse(format!("missing expression for {name}")))?;
        let env = self.current_env();
        env.assignments.borrow_mut().insert(name.clone(), node);
        env.variables.borrow_mut().insert(name, value);
        Ok(())
    }

    fn module_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let name =
            first_ident(&node).ok_or_else(|| EvalError::Parse("missing module name".to_owned()))?;
        match name.as_str() {
            "for" => self.for_call(node, parent),
            "if" => self.if_call(node, parent),
            "children" => self.children_call(node, parent),
            "let" => self.let_call(node, parent),
            "echo" => self.echo_call(node, parent),
            "assert" => self.assert_call(node, parent),
            "color" | "render" => self.pass_through_call(node, parent),
            "union" | "difference" | "intersection" if call_children(&node).is_empty() => {
                Ok(Vec::new())
            }
            _ => {
                self.call_stack.push(name.clone());
                let result = match self.lookup_module(&name) {
                    Some(declaration) => self.user_module_call(node, parent, &name, declaration),
                    None => self.module_call_one_nodes(node, parent),
                };
                self.call_stack.pop();
                result
            }
        }
    }

    fn pass_through_call(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        let mut nodes = Vec::new();
        for statement in call_children(&node) {
            nodes.extend(self.statement(statement, parent)?);
        }
        Ok(nodes)
    }

    fn modified_statement(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        let modifier = node
            .children_with_tokens()
            .find_map(|element| element.into_token())
            .map(|token| token.kind());
        if modifier == Some(SyntaxKind::STAR) {
            return Ok(Vec::new());
        }
        match node.children().next() {
            Some(child) => self.statement(child, parent),
            None => Ok(Vec::new()),
        }
    }

    fn let_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let arg_list = node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
            .ok_or_else(|| bad_args("let", "missing bindings"))?;
        let body = node
            .children()
            .find(|child| child.kind() != SyntaxKind::ARG_LIST)
            .ok_or_else(|| bad_args("let", "missing body"))?;
        self.with_let_bindings(&arg_list, |evaluator| evaluator.module_body(body, parent))
    }

    fn echo_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let args = self.arg_list_for(&node)?;
        self.messages.push(echo_message(&args));
        self.call_children(node, parent)
    }

    fn assert_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let args = self.arg_list_for(&node)?;
        check_assertion(&args).map_err(|error| self.assertion_error(&node, error))?;
        self.call_children(node, parent)
    }

    fn boolean_child_groups(
        &mut self,
        name: &str,
        block: Option<SyntaxNode>,
        node: &SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Vec<Node>>, EvalError> {
        let mut groups = Vec::new();
        if let Some(block) = block {
            for child in block.children() {
                groups.push(self.statement(child, parent)?);
            }
        } else {
            let child = single_child_statement(node)
                .ok_or_else(|| bad_args(name, "expected a block or child"))?;
            groups.push(self.statement(child, parent)?);
        }
        Ok(groups)
    }

    fn union_nodes(&mut self, name: &str, mut nodes: Vec<Node>) -> Result<Node, EvalError> {
        if nodes.is_empty() {
            return Err(bad_args(name, "group produced no geometry"));
        }
        if nodes.len() == 1 {
            return Ok(nodes.remove(0));
        }
        let dimension = nodes[0].dimension;
        if nodes.iter().any(|node| node.dimension != dimension) {
            return Err(bad_args(name, "cannot mix 2D and 3D children"));
        }
        if dimension == Dimension::TwoD {
            let mut profile = nodes[0]
                .profile
                .clone()
                .ok_or_else(|| bad_args(name, "missing 2D profile"))?;
            for node in &nodes[1..] {
                let clip = node
                    .profile
                    .as_ref()
                    .ok_or_else(|| bad_args(name, "missing 2D profile"))?;
                profile = profile::boolean(BooleanOp::Union, &profile, clip)?;
            }
            if profile.is_empty() {
                return Err(bad_args(name, "2D union produced empty geometry"));
            }
            let shape = self.kernel.sketch_profile(&profile::to_regions(&profile))?;
            let first = nodes.remove(0);
            let Node {
                module,
                source_range,
                parameters,
                world,
                ..
            } = first;
            Ok(Node {
                shape,
                module,
                source_range,
                parameters,
                transform_parameters: None,
                world,
                dimension: Dimension::TwoD,
                profile: Some(profile),
                children: nodes,
            })
        } else {
            let first = nodes.remove(0);
            let Node {
                module,
                source_range,
                parameters,
                world,
                shape: first_shape,
                ..
            } = first;
            let mut result = first_shape;
            let mut children = Vec::new();
            for node in nodes {
                result = self.kernel.boolean(BooleanOp::Union, result, node.shape)?;
                children.push(node);
            }
            Ok(Node {
                shape: result,
                module,
                source_range,
                parameters,
                transform_parameters: None,
                world,
                dimension: Dimension::ThreeD,
                profile: None,
                children,
            })
        }
    }

    fn call_children(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        let mut nodes = Vec::new();
        for child in node
            .children()
            .filter(|child| child.kind() != SyntaxKind::ARG_LIST)
        {
            nodes.extend(self.statement(child, parent)?);
        }
        Ok(nodes)
    }

    fn arg_list_for(&mut self, node: &SyntaxNode) -> Result<Arguments, EvalError> {
        match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list),
            None => Ok(Arguments::default()),
        }
    }

    fn prefixed_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let args = self.arg_list_for(node)?;
        if node.kind() == SyntaxKind::ECHO_EXPR {
            self.messages.push(echo_message(&args));
        } else {
            check_assertion(&args).map_err(|error| self.assertion_error(node, error))?;
        }

        let mut seen_keyword = false;
        let mut seen_args = false;
        let mut body = None;
        for element in node.children_with_tokens() {
            let kind = element.kind();
            if kind.is_trivia() {
                continue;
            }
            if !seen_keyword {
                seen_keyword = true;
                continue;
            }
            if !seen_args {
                if kind == SyntaxKind::ARG_LIST {
                    seen_args = true;
                    continue;
                }
                body = Some(element);
                break;
            }
            body = Some(element);
            break;
        }
        match body {
            Some(body) => self.element_value(body),
            None => Ok(Value::Undef),
        }
    }

    fn dot_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut base = None;
        let mut member = None;
        let mut after_dot = false;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::DOT => after_dot = true,
                kind if kind.is_trivia() => {}
                _ if after_dot => {
                    member = element.into_token().map(|token| token.text().to_owned());
                }
                _ => base = Some(element),
            }
        }

        let base = self.element_value(
            base.ok_or_else(|| EvalError::Expression("missing dotted value".to_owned()))?,
        )?;
        let index = match member.as_deref() {
            Some("x") => 0,
            Some("y") => 1,
            Some("z") => 2,
            _ => return Ok(Value::Undef),
        };
        match base {
            Value::List(values) => Ok(values.get(index).cloned().unwrap_or(Value::Undef)),
            _ => Ok(Value::Undef),
        }
    }

    fn user_module_call(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
        name: &str,
        declaration: Declaration,
    ) -> Result<Vec<Node>, EvalError> {
        let declaration_node = declaration.node.clone();
        let args = match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        let parameters = declaration_node
            .children()
            .find(|child| child.kind() == SyntaxKind::PARAM_LIST)
            .map(|list| declaration_parameters(&list))
            .unwrap_or_default();
        for named in args.named.keys() {
            if !parameters.iter().any(|(param, _)| param == named) {
                return Err(bad_args(
                    name,
                    format!("unsupported named argument {named}"),
                ));
            }
        }
        if args.positional.len() > parameters.len() {
            return Err(bad_args(name, "too many arguments"));
        }

        let mut bindings = HashMap::new();
        for (index, value) in args.positional.iter().enumerate() {
            bindings.insert(parameters[index].0.clone(), value.clone());
        }
        for (param, value) in &args.named {
            bindings.insert(param.clone(), value.clone());
        }

        let statements = call_children(&node);
        let caller_parameters = node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
            .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
            .unwrap_or_default();
        let caller_env = self.current_env();
        let special = args.special.clone();
        let call_env = Env::child(declaration.env.clone(), caller_env.clone());

        let mut nodes = self.with_env(call_env.clone(), |evaluator| {
            {
                let mut variables = call_env.variables.borrow_mut();
                for (name, value) in &special {
                    variables.insert(name.clone(), value.clone());
                }
                variables.insert(
                    "$children".to_owned(),
                    Value::Number(statements.len() as f64),
                );
            }

            for (param, default) in &parameters {
                let value = match bindings.get(param) {
                    Some(value) => value.clone(),
                    None => match default {
                        Some(expression) => evaluator.element_value(expression.clone())?,
                        None => Value::Undef,
                    },
                };
                call_env.variables.borrow_mut().insert(param.clone(), value);
            }
            evaluator.children_stack.push(ChildrenFrame {
                statements,
                caller_env,
                special,
            });
            let body = declaration_node
                .children()
                .find(|child| {
                    matches!(
                        child.kind(),
                        SyntaxKind::BLOCK | SyntaxKind::MODULE_CALL | SyntaxKind::ASSIGNMENT
                    )
                })
                .ok_or_else(|| bad_args(name, "missing module body"));
            let result = body.and_then(|body| evaluator.module_body(body, parent));
            evaluator.children_stack.pop();
            result
        })?;

        for node in &mut nodes {
            merge_parameters(node, &caller_parameters);
        }
        Ok(nodes)
    }

    fn children_call(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        let args = match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        args.ensure_known("children", &[])?;
        if args.positional.len() > 1 {
            return Err(bad_args("children", "expected at most one index"));
        }
        let index = args.positional.first();
        let (statements, caller_env, special) = match self.children_stack.last() {
            Some(frame) => (
                frame.statements.clone(),
                frame.caller_env.clone(),
                frame.special.clone(),
            ),
            None => return Err(bad_args("children", "not inside a module")),
        };

        let selected = match index {
            None => statements,
            Some(Value::Number(index)) => {
                let index = child_index(*index)?;
                statements
                    .into_iter()
                    .nth(index)
                    .map(|statement| vec![statement])
                    .ok_or_else(|| bad_args("children", format!("no child at index {index}")))?
            }
            Some(Value::List(indices)) => {
                let mut selected = Vec::new();
                for value in indices {
                    let index = child_index(value.as_number("children")?)?;
                    let statement = statements.get(index).cloned().ok_or_else(|| {
                        bad_args("children", format!("no child at index {index}"))
                    })?;
                    selected.push(statement);
                }
                selected
            }
            Some(_) => {
                return Err(bad_args(
                    "children",
                    "index must be a number or a list of numbers",
                ));
            }
        };

        let body_env = self.current_env();
        let frame = self
            .children_stack
            .pop()
            .expect("children frame checked above");
        let env = Env::child(caller_env, body_env);
        if !special.is_empty() {
            env.variables.borrow_mut().extend(special);
        }
        let saved = std::mem::replace(&mut self.scopes, vec![env]);
        let result = (|| {
            let mut nodes = Vec::new();
            for statement in selected {
                nodes.extend(self.statement(statement, parent)?);
            }
            Ok(nodes)
        })();
        self.scopes = saved;
        self.children_stack.push(frame);
        result
    }

    fn module_body(&mut self, body: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        if body.kind() == SyntaxKind::BLOCK {
            self.block(body, parent)
        } else {
            self.statement(body, parent)
        }
    }

    fn current_env(&self) -> Rc<Env> {
        self.scopes.last().expect("scope").clone()
    }

    fn with_env<T>(
        &mut self,
        env: Rc<Env>,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(EvalError::Expression(format!(
                "maximum call depth exceeded at {}",
                self.call_stack.join(" > ")
            )));
        }
        self.call_depth += 1;
        self.scopes.push(env);
        let result = f(self);
        self.scopes.pop();
        self.call_depth -= 1;
        result
    }

    fn with_child_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        let parent = self.current_env();
        self.with_env(Env::child(parent.clone(), parent), f)
    }

    fn register_declarations(&mut self, node: &SyntaxNode) {
        let modules = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::MODULE_DECLARATION)
            .filter_map(|child| module_name(&child).map(|name| (name, child)))
            .collect::<Vec<_>>();
        let functions = node
            .children()
            .filter(|child| child.kind() == SyntaxKind::FUNCTION_DECLARATION)
            .filter_map(|child| module_name(&child).map(|name| (name, child)))
            .collect::<Vec<_>>();
        let env = self.current_env();
        {
            let mut registered = env.modules.borrow_mut();
            for (name, node) in modules {
                registered.insert(
                    name,
                    Declaration {
                        node,
                        env: env.clone(),
                    },
                );
            }
        }
        {
            let mut registered = env.functions.borrow_mut();
            for (name, node) in functions {
                registered.insert(
                    name,
                    Declaration {
                        node,
                        env: env.clone(),
                    },
                );
            }
        }
    }

    fn lookup_module(&self, name: &str) -> Option<Declaration> {
        let mut env = Some(self.current_env());
        while let Some(current) = env {
            if let Some(declaration) = current.modules.borrow().get(name) {
                return Some(declaration.clone());
            }
            env = current.lexical_parent.clone();
        }
        None
    }

    fn lookup_function(&self, name: &str) -> Option<Declaration> {
        let mut env = Some(self.current_env());
        while let Some(current) = env {
            if let Some(declaration) = current.functions.borrow().get(name) {
                return Some(declaration.clone());
            }
            env = current.lexical_parent.clone();
        }
        None
    }

    fn evaluate_source_tree(
        &mut self,
        root: &SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        self.register_declarations_deep(root)?;
        self.apply_included_assignments_deep(root)?;
        self.apply_assignments(root)?;
        let mut nodes = Vec::new();
        for statement in root.children() {
            if matches!(statement.kind(), SyntaxKind::ASSIGNMENT | SyntaxKind::USE) {
                continue;
            }
            nodes.extend(self.statement(statement, parent)?);
        }
        Ok(nodes)
    }

    fn include_file(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
        use_only: bool,
    ) -> Result<Vec<Node>, EvalError> {
        let (request, angle) = include_path(&node)
            .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
        let path = self.resolve_source(&request, angle)?;
        let source = std::fs::read_to_string(&path).map_err(|error| {
            EvalError::Source(format!("failed to read {}: {error}", path.display()))
        })?;
        let parsed = parse(&source);
        if !parsed.errors().is_empty() {
            return Err(parse_error(&path, &parsed));
        }
        let root = parsed.syntax();

        self.with_source_file(path, |evaluator| {
            if use_only {
                evaluator.use_declarations(&root)?;
                Ok(Vec::new())
            } else {
                evaluator.evaluate_source_tree(&root, parent)
            }
        })
    }

    fn register_declarations_deep(&mut self, root: &SyntaxNode) -> Result<(), EvalError> {
        self.register_declarations(root);
        for statement in root.children() {
            if !matches!(statement.kind(), SyntaxKind::INCLUDE | SyntaxKind::USE) {
                continue;
            }
            let use_only = statement.kind() == SyntaxKind::USE;
            let (request, angle) = include_path(&statement)
                .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
            let path = self.resolve_source(&request, angle)?;
            let source = std::fs::read_to_string(&path).map_err(|error| {
                EvalError::Source(format!("failed to read {}: {error}", path.display()))
            })?;
            let parsed = parse(&source);
            if !parsed.errors().is_empty() {
                return Err(parse_error(&path, &parsed));
            }
            let child = parsed.syntax();
            self.with_source_file(path, |evaluator| {
                if use_only {
                    evaluator.use_declarations(&child)
                } else {
                    evaluator.register_declarations_deep(&child)
                }
            })?;
        }
        Ok(())
    }

    fn apply_included_assignments_deep(&mut self, root: &SyntaxNode) -> Result<(), EvalError> {
        for statement in root.children() {
            if statement.kind() != SyntaxKind::INCLUDE {
                continue;
            }
            let (request, angle) = include_path(&statement)
                .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
            let path = self.resolve_source(&request, angle)?;
            let source = std::fs::read_to_string(&path).map_err(|error| {
                EvalError::Source(format!("failed to read {}: {error}", path.display()))
            })?;
            let parsed = parse(&source);
            if !parsed.errors().is_empty() {
                return Err(parse_error(&path, &parsed));
            }
            let child = parsed.syntax();
            self.with_source_file(path, |evaluator| {
                evaluator.apply_source_assignments_in_order(&child)
            })?;
        }
        Ok(())
    }

    fn apply_source_assignments_in_order(&mut self, root: &SyntaxNode) -> Result<(), EvalError> {
        for statement in root.children() {
            match statement.kind() {
                SyntaxKind::ASSIGNMENT => self.assignment(statement)?,
                SyntaxKind::INCLUDE => {
                    let (request, angle) = include_path(&statement)
                        .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
                    let path = self.resolve_source(&request, angle)?;
                    let source = std::fs::read_to_string(&path).map_err(|error| {
                        EvalError::Source(format!("failed to read {}: {error}", path.display()))
                    })?;
                    let parsed = parse(&source);
                    if !parsed.errors().is_empty() {
                        return Err(parse_error(&path, &parsed));
                    }
                    let child = parsed.syntax();
                    self.with_source_file(path, |evaluator| {
                        evaluator.apply_source_assignments_in_order(&child)
                    })?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn use_declarations(&mut self, root: &SyntaxNode) -> Result<(), EvalError> {
        let used = Env::root();
        self.with_env(used.clone(), |evaluator| {
            evaluator.register_declarations_deep(root)
        })?;
        let current = self.current_env();
        current.modules.borrow_mut().extend(
            used.modules
                .borrow()
                .iter()
                .map(|(name, declaration)| (name.clone(), declaration.clone())),
        );
        current.functions.borrow_mut().extend(
            used.functions
                .borrow()
                .iter()
                .map(|(name, declaration)| (name.clone(), declaration.clone())),
        );
        Ok(())
    }

    fn resolve_source(&self, request: &str, angle: bool) -> Result<PathBuf, EvalError> {
        let path = Path::new(request);
        if path.is_absolute() {
            return Ok(canonical_path(path));
        }

        let mut candidates = Vec::new();
        if let Some(source_dir) = &self.source_dir {
            candidates.push(source_dir.join(path));
        }
        if angle {
            candidates.extend(self.library_paths.iter().map(|dir| dir.join(path)));
        }
        if candidates.is_empty() {
            return Err(EvalError::Source(format!(
                "include \"{request}\" requires a source file or library path"
            )));
        }

        for candidate in &candidates {
            if candidate.is_file() {
                return Ok(canonical_path(candidate));
            }
        }
        Ok(canonical_path(&candidates[0]))
    }

    fn with_source_file<T>(
        &mut self,
        path: PathBuf,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        if self.include_stack.iter().any(|entry| entry == &path) {
            return Err(EvalError::Source(format!(
                "include cycle: {}",
                path.display()
            )));
        }
        let previous_dir =
            std::mem::replace(&mut self.source_dir, path.parent().map(Path::to_path_buf));
        self.include_stack.push(path);
        let result = f(self);
        self.include_stack.pop();
        self.source_dir = previous_dir;
        result
    }

    fn assignment_view(&self) -> HashMap<String, SyntaxNode> {
        let mut assignments = HashMap::new();
        let mut env = Some(self.current_env());
        while let Some(current) = env {
            assignments.extend(
                current
                    .assignments
                    .borrow()
                    .iter()
                    .map(|(name, node)| (name.clone(), node.clone())),
            );
            env = current.lexical_parent.clone();
        }
        assignments
    }

    fn if_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let arg_list = node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
            .ok_or_else(|| bad_args("if", "expected a condition"))?;
        let args = self.arg_list(arg_list)?;
        if !args.named.is_empty() || args.positional.len() != 1 {
            return Err(bad_args("if", "expected one condition"));
        }
        let condition = args.positional[0].as_boolean("if")?;

        let then_body = node
            .children()
            .find(|child| matches!(child.kind(), SyntaxKind::BLOCK | SyntaxKind::MODULE_CALL));
        let else_body = node
            .children()
            .find(|child| child.kind() == SyntaxKind::ELSE)
            .and_then(|else_node| {
                else_node.children().find(|child| {
                    matches!(child.kind(), SyntaxKind::BLOCK | SyntaxKind::MODULE_CALL)
                })
            });

        if condition {
            let body = then_body.ok_or_else(|| bad_args("if", "expected a body"))?;
            self.eval_control_body(body, parent)
        } else if let Some(body) = else_body {
            self.eval_control_body(body, parent)
        } else {
            Ok(Vec::new())
        }
    }

    fn for_call(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        let arg_list = node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
            .ok_or_else(|| bad_args("for", "expected loop bindings"))?;
        let bindings = self.comprehension_bindings(&arg_list)?;
        if bindings.is_empty() {
            return Err(bad_args("for", "expected at least one loop binding"));
        }

        let body = node
            .children()
            .find(|child| matches!(child.kind(), SyntaxKind::BLOCK | SyntaxKind::MODULE_CALL))
            .ok_or_else(|| bad_args("for", "expected a body"))?;
        let mut nodes = Vec::new();
        self.for_bindings(&bindings, 0, &body, parent, &mut nodes)?;
        Ok(nodes)
    }

    fn for_bindings(
        &mut self,
        bindings: &[(String, SyntaxElement)],
        index: usize,
        body: &SyntaxNode,
        parent: Transform,
        nodes: &mut Vec<Node>,
    ) -> Result<(), EvalError> {
        if index == bindings.len() {
            nodes.extend(self.eval_control_body(body.clone(), parent)?);
            return Ok(());
        }

        let (name, expression) = &bindings[index];
        let value = self.element_value(expression.clone())?;
        let values = loop_values(name, value)?;
        let mut outcome = Ok(());
        for value in values {
            let env_parent = self.current_env();
            let env = Env::child(env_parent.clone(), env_parent);
            env.variables.borrow_mut().insert(name.clone(), value);
            self.scopes.push(env);
            outcome = self.for_bindings(bindings, index + 1, body, parent, nodes);
            self.scopes.pop();
            if outcome.is_err() {
                break;
            }
        }
        outcome
    }

    fn eval_control_body(
        &mut self,
        body: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        if body.kind() == SyntaxKind::BLOCK {
            self.block(body, parent)
        } else {
            self.statement(body, parent)
        }
    }

    fn module_call_one_nodes(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        match self.module_call_one(node, parent) {
            Ok(node) => Ok(vec![node]),
            Err(EvalError::BadArguments { module, message })
                if matches!(
                    module.as_str(),
                    "translate"
                        | "rotate"
                        | "scale"
                        | "multmatrix"
                        | "mirror"
                        | "hull"
                        | "minkowski"
                ) && matches!(
                    message.as_str(),
                    "expected at least one child" | "expected a child or block"
                ) =>
            {
                // OpenSCAD treats an empty transform as an empty subtree.
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    fn module_call_one(&mut self, node: SyntaxNode, parent: Transform) -> Result<Node, EvalError> {
        let name =
            first_ident(&node).ok_or_else(|| EvalError::Parse("missing module name".to_owned()))?;
        let module = name.clone();
        let source_range = text_range_to_range(node.text_range());
        let parameters = collect_parameters(&node, &self.assignment_view());

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
                args.ensure_known("cube", &["size", "center"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("cube", "expected size and optional center"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("cube", "expected size"))?
                    .as_size("cube")?;
                let center = args
                    .argument("center", 1)
                    .map(|value| value.as_boolean("cube"))
                    .transpose()?;
                let local =
                    self.placement_offset([size[0] / 2.0, size[1] / 2.0, size[2] / 2.0], center);
                let world = parent.compose(local);
                let shape = self.kernel.primitive(Primitive::Cuboid { size })?;
                let shape = self.apply_transform(shape, world)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children: Vec::new(),
                })
            }
            "cylinder" => {
                args.ensure_known(
                    "cylinder",
                    &["r", "r1", "r2", "d", "d1", "d2", "h", "center"],
                )?;
                if args.positional.len() > 4 {
                    return Err(bad_args("cylinder", "too many positional arguments"));
                }

                let named_radius = |name: &str| -> Result<Option<f64>, EvalError> {
                    match args.named.get(name) {
                        None | Some(Value::Undef) => Ok(None),
                        Some(value) => Ok(Some(value.as_number("cylinder")?)),
                    }
                };
                let positional_radius = |index: usize| -> Result<Option<f64>, EvalError> {
                    match args.positional.get(index) {
                        Some(value) => Ok(Some(value.as_number("cylinder")?)),
                        None => Ok(None),
                    }
                };
                let base = named_radius("r")?.or(named_radius("d")?.map(|diameter| diameter / 2.0));
                let radius1 = named_radius("r1")?
                    .or(named_radius("d1")?.map(|diameter| diameter / 2.0))
                    .or(positional_radius(1)?)
                    .or(base)
                    .unwrap_or(1.0);
                let radius2 = named_radius("r2")?
                    .or(named_radius("d2")?.map(|diameter| diameter / 2.0))
                    .or(positional_radius(2)?)
                    .or(base)
                    .unwrap_or(radius1);
                let height_value = match args.named.get("h") {
                    Some(Value::Undef) | None => args.positional.first(),
                    Some(value) => Some(value),
                };
                let height = height_value
                    .ok_or_else(|| bad_args("cylinder", "expected height"))?
                    .as_number("cylinder")?;
                if !radius1.is_finite() || radius1 < 0.0 {
                    return Err(bad_args("cylinder", "r1 must be a non-negative number"));
                }
                if !radius2.is_finite() || radius2 < 0.0 {
                    return Err(bad_args("cylinder", "r2 must be a non-negative number"));
                }
                if !height.is_finite() || height <= 0.0 {
                    return Err(bad_args("cylinder", "h must be a positive number"));
                }
                if radius1 == 0.0 && radius2 == 0.0 {
                    return Err(bad_args("cylinder", "cylinder radii cannot both be zero"));
                }

                let center_value = match args.named.get("center") {
                    Some(Value::Undef) | None => args.positional.get(3),
                    Some(value) => Some(value),
                };
                let center = center_value
                    .map(|value| value.as_boolean("cylinder"))
                    .transpose()?;
                let local = self.placement_offset([0.0, 0.0, height / 2.0], center);
                let world = parent.compose(local);
                let shape = if radius1 == radius2 {
                    match self.fn_segments(&args)? {
                        Some(segments) => {
                            let profile = profile::circle_with_segments(radius1, segments);
                            let sketch =
                                self.kernel.sketch_profile(&profile::to_regions(&profile))?;
                            self.kernel.extrude(sketch, height)?
                        }
                        None => self.kernel.primitive(Primitive::Cylinder {
                            radius: radius1,
                            height,
                        })?,
                    }
                } else {
                    let segments = self.fn_segments(&args)?.unwrap_or(64).max(3);
                    let (points, faces) = cylinder_frustum(radius1, radius2, height, segments);
                    self.kernel
                        .primitive(Primitive::Polyhedron { points, faces })?
                };
                let shape = self.apply_transform(shape, world)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children: Vec::new(),
                })
            }
            "sphere" => {
                args.ensure_known("sphere", &["r", "d"])?;
                let radius = args.radius("sphere")?;
                let segments = self.fn_segments(&args)?;
                let shape = self
                    .kernel
                    .primitive(Primitive::Sphere { radius, segments })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children: Vec::new(),
                })
            }
            "polyhedron" => {
                args.ensure_known("polyhedron", &["points", "faces", "triangles", "convexity"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("polyhedron", "expected points and faces"));
                }
                let points = args
                    .argument("points", 0)
                    .ok_or_else(|| bad_args("polyhedron", "expected points"))?
                    .as_points3("polyhedron")?;
                let faces = args
                    .argument("faces", 1)
                    .or_else(|| args.named.get("triangles"))
                    .ok_or_else(|| bad_args("polyhedron", "expected faces"))?
                    .as_faces("polyhedron", points.len())?;
                let shape = self
                    .kernel
                    .primitive(Primitive::Polyhedron { points, faces })?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children: Vec::new(),
                })
            }
            "square" => {
                args.ensure_known("square", &["size", "center"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("square", "expected size and optional center"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("square", "expected size"))?
                    .as_size2("square")?;
                if size.iter().any(|value| *value <= 0.0) {
                    return Err(bad_args("square", "dimensions must be positive"));
                }
                let center = match args.named.get("center").or_else(|| args.positional.get(1)) {
                    None | Some(Value::Undef) => None,
                    Some(value) => Some(value.as_boolean("square")?),
                };
                let local = self.placement_offset([size[0] / 2.0, size[1] / 2.0, 0.0], center);
                let world = parent.compose(local);
                let profile = profile::square(size);
                let profile = profile::transform(&profile, world);
                let shape = self.kernel.sketch(Sketch::Square { size })?;
                let shape = self.apply_transform(shape, world)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world,
                    dimension: Dimension::TwoD,
                    profile: Some(profile),
                    children: Vec::new(),
                })
            }
            "circle" => {
                args.ensure_known("circle", &["r", "d"])?;
                let radius = args.radius("circle")?;
                if radius <= 0.0 {
                    return Err(bad_args("circle", "radius must be positive"));
                }
                let segments = self.fn_segments(&args)?;
                let base_profile = match segments {
                    Some(segments) => profile::circle_with_segments(radius, segments),
                    None => {
                        profile::circle_with_segments(radius, self.profile_segments(&args, radius)?)
                    }
                };
                let shape = match segments {
                    Some(_) => self.kernel.sketch(Sketch::Polygon {
                        points: base_profile[0].outer.clone(),
                    })?,
                    None => self.kernel.sketch(Sketch::Circle { radius })?,
                };
                let shape = self.apply_transform(shape, parent)?;
                let profile = profile::transform(&base_profile, parent);
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::TwoD,
                    profile: Some(profile),
                    children: Vec::new(),
                })
            }
            "polygon" => {
                args.ensure_known("polygon", &["points", "paths", "convexity"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("polygon", "expected one point list"));
                }
                let points = args
                    .argument("points", 0)
                    .ok_or_else(|| bad_args("polygon", "expected points"))?
                    .as_polygon_points("polygon")?;
                if points.len() < 3 {
                    return Err(bad_args("polygon", "expected at least three points"));
                }
                let paths = match args.named.get("paths") {
                    None | Some(Value::Undef) => None,
                    Some(value) => {
                        let Value::List(paths) = value else {
                            return Err(bad_args(
                                "polygon",
                                "paths must be a list of point indices",
                            ));
                        };
                        let mut result = Vec::with_capacity(paths.len());
                        for path in paths {
                            let Value::List(indices) = path else {
                                return Err(bad_args(
                                    "polygon",
                                    "each path must be a list of point indices",
                                ));
                            };
                            let mut path_indices = Vec::with_capacity(indices.len());
                            for index in indices {
                                let index = index.as_number("polygon")?;
                                if !index.is_finite()
                                    || index < 0.0
                                    || index.fract() != 0.0
                                    || index as usize >= points.len()
                                {
                                    return Err(bad_args("polygon", "invalid point index"));
                                }
                                path_indices.push(index as usize);
                            }
                            result.push(path_indices);
                        }
                        Some(result)
                    }
                };

                let (profile, shape) = match paths {
                    None => {
                        let profile = profile::polygon(&points);
                        let shape = self.kernel.sketch(Sketch::Polygon { points })?;
                        (profile, shape)
                    }
                    Some(paths) => {
                        if paths.is_empty() {
                            return Err(bad_args("polygon", "paths cannot be empty"));
                        }
                        let mut profile = Vec::new();
                        for path in &paths {
                            if path.len() < 3 {
                                return Err(bad_args(
                                    "polygon",
                                    "each path needs at least three points",
                                ));
                            }
                            let path_points =
                                path.iter().map(|index| points[*index]).collect::<Vec<_>>();
                            profile.extend(profile::polygon(&path_points));
                        }
                        let shape = self.kernel.sketch_profile(&profile::to_regions(&profile))?;
                        (profile, shape)
                    }
                };
                let profile = profile::transform(&profile, parent);
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::TwoD,
                    profile: Some(profile),
                    children: Vec::new(),
                })
            }
            "offset" => {
                args.ensure_known("offset", &["r", "delta", "chamfer"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("offset", "expected one distance"));
                }
                let chamfer = args
                    .named
                    .get("chamfer")
                    .map(|value| value.as_boolean("offset"))
                    .transpose()?
                    .unwrap_or(false);
                let r = args
                    .named
                    .get("r")
                    .map(|value| value.as_number("offset"))
                    .transpose()?;
                let delta = args
                    .named
                    .get("delta")
                    .map(|value| value.as_number("offset"))
                    .transpose()?;
                let positional = args
                    .positional
                    .first()
                    .map(|value| value.as_number("offset"))
                    .transpose()?;

                let (distance, round) = match (r, delta, positional) {
                    (Some(r), None, None) => (r, true),
                    (None, Some(delta), None) => (delta, false),
                    (None, None, Some(distance)) => (distance, true),
                    (None, None, None) => {
                        return Err(bad_args("offset", "expected r or delta"));
                    }
                    _ => {
                        return Err(bad_args(
                            "offset",
                            "specify exactly one of r, delta, or a positional distance",
                        ));
                    }
                };
                if chamfer && !round {
                    return Err(bad_args("offset", "chamfer requires r"));
                }
                let join = match (round, chamfer) {
                    (true, true) => profile::OffsetJoin::Bevel,
                    (true, false) => profile::OffsetJoin::Round,
                    (false, _) => profile::OffsetJoin::Miter,
                };

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
                    .unwrap_or_default();

                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, Transform::IDENTITY),
                    None => {
                        let child = single_child_statement(&node)
                            .ok_or_else(|| bad_args("offset", "expected a child or block"))?;
                        evaluator.statement(child, Transform::IDENTITY)
                    }
                })?;
                if children.len() != 1 {
                    return Err(bad_args("offset", "expected exactly one 2D child"));
                }
                if children[0].dimension != Dimension::TwoD {
                    return Err(bad_args("offset", "expected a 2D child"));
                }

                let profile = children[0]
                    .profile
                    .as_ref()
                    .ok_or_else(|| bad_args("offset", "missing 2D profile"))?;
                let offset_profile = profile::offset(profile, distance, join)?;
                if offset_profile.is_empty() {
                    return Err(bad_args("offset", "offset produced empty geometry"));
                }

                let shape = self
                    .kernel
                    .sketch_profile(&profile::to_regions(&offset_profile))?;
                let shape = self.apply_transform(shape, parent)?;
                let profile = profile::transform(&offset_profile, parent);
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: parent,
                    dimension: Dimension::TwoD,
                    profile: Some(profile),
                    children,
                })
            }
            "linear_extrude" => {
                args.ensure_known(
                    "linear_extrude",
                    &[
                        "height",
                        "center",
                        "convexity",
                        "twist",
                        "slices",
                        "scale",
                        "segments",
                    ],
                )?;
                if args.positional.len() > 2 {
                    return Err(bad_args("linear_extrude", "expected height and center"));
                }
                let height = args
                    .argument("height", 0)
                    .ok_or_else(|| bad_args("linear_extrude", "expected height"))?
                    .as_number("linear_extrude")?;
                let center = args
                    .argument("center", 1)
                    .map(|value| value.as_boolean("linear_extrude"))
                    .transpose()?;
                let twist = match args.named.get("twist") {
                    None | Some(Value::Undef) => 0.0,
                    Some(value) => value.as_number("linear_extrude")?,
                };
                let scale = match args.named.get("scale") {
                    None | Some(Value::Undef) => [1.0, 1.0],
                    Some(value) => value.as_size2("linear_extrude")?,
                };
                let slices = match args.named.get("slices") {
                    None | Some(Value::Undef) => {
                        if twist.abs() > f64::EPSILON {
                            ((twist.abs() / 15.0).ceil() as usize).max(2)
                        } else {
                            2
                        }
                    }
                    Some(value) => {
                        let value = value.as_number("linear_extrude")?;
                        if value < 2.0 || value.fract() != 0.0 {
                            return Err(bad_args(
                                "linear_extrude",
                                "slices must be an integer of at least 2",
                            ));
                        }
                        value as usize
                    }
                };
                if !twist.is_finite() {
                    return Err(bad_args("linear_extrude", "twist must be finite"));
                }
                if scale.iter().any(|value| !value.is_finite()) {
                    return Err(bad_args("linear_extrude", "scale must be finite"));
                }

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
                    .unwrap_or_default();

                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, Transform::IDENTITY),
                    None => {
                        let child = single_child_statement(&node).ok_or_else(|| {
                            bad_args("linear_extrude", "expected a child or block")
                        })?;
                        evaluator.statement(child, Transform::IDENTITY)
                    }
                })?;
                if children.is_empty() {
                    return Err(bad_args("linear_extrude", "expected a 2D child"));
                }
                if children
                    .iter()
                    .any(|child| child.dimension != Dimension::TwoD)
                {
                    return Err(bad_args("linear_extrude", "expected 2D children"));
                }

                let mut profile = children[0]
                    .profile
                    .clone()
                    .ok_or_else(|| bad_args("linear_extrude", "missing 2D profile"))?;
                for child in &children[1..] {
                    let clip = child
                        .profile
                        .as_ref()
                        .ok_or_else(|| bad_args("linear_extrude", "missing 2D profile"))?;
                    profile = profile::boolean(BooleanOp::Union, &profile, clip)?;
                }
                if profile.is_empty() {
                    return Err(bad_args(
                        "linear_extrude",
                        "2D children produced empty geometry",
                    ));
                }
                let source = if children.len() == 1 {
                    children[0].shape
                } else {
                    self.kernel.sketch_profile(&profile::to_regions(&profile))?
                };

                let shape = if twist.abs() > f64::EPSILON
                    || (scale[0] - 1.0).abs() > f64::EPSILON
                    || (scale[1] - 1.0).abs() > f64::EPSILON
                {
                    let (points, faces) = extrude::loft(&profile, height, twist, scale, slices)
                        .ok_or_else(|| {
                            bad_args(
                                "linear_extrude",
                                "twist/scale requires a single contour without holes",
                            )
                        })?;
                    self.kernel
                        .primitive(Primitive::Polyhedron { points, faces })?
                } else {
                    self.kernel.extrude(source, height)?
                };
                let local = match center {
                    Some(true) => Transform::IDENTITY,
                    Some(false) => Transform::from_translation([0.0, 0.0, height / 2.0]),
                    None => self.centering_offset([0.0, 0.0, height / 2.0]),
                };
                let world = parent.compose(local);
                let shape = self.apply_transform(shape, world)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children,
                })
            }
            "rotate_extrude" => {
                args.ensure_known("rotate_extrude", &["angle", "start", "convexity"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("rotate_extrude", "expected an angle and start"));
                }
                let angle = match args.argument("angle", 0) {
                    None | Some(Value::Undef) => 360.0,
                    Some(value) => value.as_number("rotate_extrude")?,
                };
                let start = match args.argument("start", 1) {
                    None | Some(Value::Undef) => 0.0,
                    Some(value) => value.as_number("rotate_extrude")?,
                };
                if !start.is_finite() {
                    return Err(bad_args("rotate_extrude", "start must be finite"));
                }

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
                    .unwrap_or_default();

                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, Transform::IDENTITY),
                    None => {
                        let child = single_child_statement(&node).ok_or_else(|| {
                            bad_args("rotate_extrude", "expected a child or block")
                        })?;
                        evaluator.statement(child, Transform::IDENTITY)
                    }
                })?;
                if children.len() != 1 {
                    return Err(bad_args("rotate_extrude", "expected exactly one 2D child"));
                }
                if children[0].dimension != Dimension::TwoD {
                    return Err(bad_args("rotate_extrude", "expected a 2D child"));
                }

                let (min, max) = self
                    .kernel
                    .bounds(children[0].shape, self.tolerance)
                    .map_err(|_| bad_args("rotate_extrude", "cannot revolve an empty profile"))?;
                if min[0] < -1.0e-6 {
                    return Err(bad_args("rotate_extrude", "profile must lie at x >= 0"));
                }
                if max[0] <= 1.0e-6 {
                    return Err(bad_args(
                        "rotate_extrude",
                        "profile must not lie on the axis",
                    ));
                }

                let shape = self.kernel.revolve(
                    children[0].shape,
                    start.to_radians(),
                    angle.to_radians(),
                )?;
                let shape = self.apply_transform(shape, parent)?;
                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: parent,
                    dimension: Dimension::ThreeD,
                    profile: None,
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
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
                    .unwrap_or_default();

                let mut children = match block {
                    Some(block) => self.block(block, Transform::IDENTITY)?,
                    None => {
                        let child = single_child_statement(&node)
                            .ok_or_else(|| bad_args("resize", "expected a child or block"))?;
                        self.statement(child, Transform::IDENTITY)?
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
                let (min, max) = self
                    .kernel
                    .bounds(united, self.tolerance)
                    .map_err(|_| bad_args("resize", "cannot resize empty geometry"))?;

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
                let profile = if dimension == Dimension::TwoD {
                    Some(
                        children[0]
                            .profile
                            .clone()
                            .ok_or_else(|| bad_args("resize", "missing profile for 2D child"))?,
                    )
                } else {
                    None
                };

                Ok(Node {
                    shape: result,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: combined,
                    dimension,
                    profile,
                    children,
                })
            }
            "translate" | "rotate" | "scale" | "multmatrix" | "mirror" => {
                let local = if name == "rotate" {
                    self.rotate_transform(&args)?
                } else {
                    let first = match name.as_str() {
                        "multmatrix" => "m",
                        _ => "v",
                    };
                    args.ensure_known(name.as_str(), &[first])?;
                    let argument = args.one(name.as_str(), first)?;
                    match name.as_str() {
                        "translate" => {
                            let vector = match argument {
                                Value::List(values) if values.len() == 2 => [
                                    values[0].as_number("translate")?,
                                    values[1].as_number("translate")?,
                                    0.0,
                                ],
                                _ => argument.as_vector3("translate")?,
                            };
                            Transform::from_translation(vector)
                        }
                        "multmatrix" => argument.as_matrix("multmatrix")?,
                        "mirror" => {
                            let normal = argument.as_vector3("mirror")?;
                            Transform::from_mirror(normal).ok_or_else(|| {
                                bad_args("mirror", "normal must be non-zero and finite")
                            })?
                        }
                        _ => Transform::from_scale(argument.as_scale("scale")?),
                    }
                };
                let combined = parent.compose(local);

                let transform_parameters = node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::ARG_LIST)
                    .map(|arg_list| collect_parameters(&arg_list, &self.assignment_view()))
                    .unwrap_or_default();

                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, combined),
                    None => {
                        let child = single_child_statement(&node)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a child or block"))?;
                        evaluator.statement(child, combined)
                    }
                })?;
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }
                let dimension = children[0].dimension;
                if children.iter().any(|child| child.dimension != dimension) {
                    return Err(bad_args(name.as_str(), "cannot mix 2D and 3D children"));
                }

                let (result, profile) = if dimension == Dimension::TwoD {
                    let mut profile = children[0]
                        .profile
                        .clone()
                        .ok_or_else(|| bad_args(name.as_str(), "missing 2D profile"))?;
                    for child in &children[1..] {
                        let clip = child
                            .profile
                            .as_ref()
                            .ok_or_else(|| bad_args(name.as_str(), "missing 2D profile"))?;
                        profile = profile::boolean(BooleanOp::Union, &profile, clip)?;
                    }
                    if profile.is_empty() {
                        return Err(bad_args(name.as_str(), "2D union produced empty geometry"));
                    }
                    let result = if children.len() == 1 {
                        children[0].shape
                    } else {
                        self.kernel.sketch_profile(&profile::to_regions(&profile))?
                    };
                    (result, Some(profile))
                } else {
                    let mut shapes = children.iter().map(|child| child.shape);
                    let mut result = shapes.next().expect("checked non-empty");
                    for shape in shapes {
                        result = self.kernel.boolean(BooleanOp::Union, result, shape)?;
                    }
                    (result, None)
                };

                Ok(Node {
                    shape: result,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: Some(transform_parameters),
                    world: combined,
                    dimension,
                    profile,
                    children,
                })
            }
            "difference" | "union" | "intersection" => {
                args.ensure_empty(name.as_str())?;
                let groups = self.with_special_overrides(&args, |evaluator| {
                    evaluator.boolean_child_groups(name.as_str(), block, &node, parent)
                })?;
                let mut children = Vec::new();
                for group in groups {
                    if group.is_empty() {
                        continue;
                    }
                    children.push(self.union_nodes(name.as_str(), group)?);
                }
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }
                let dimension = children[0].dimension;
                if children.iter().any(|child| child.dimension != dimension) {
                    return Err(bad_args(name.as_str(), "cannot mix 2D and 3D children"));
                }

                let op = match name.as_str() {
                    "difference" => BooleanOp::Difference,
                    "union" => BooleanOp::Union,
                    _ => BooleanOp::Intersection,
                };

                if dimension == Dimension::TwoD {
                    let mut profile = children[0]
                        .profile
                        .clone()
                        .ok_or_else(|| bad_args(name.as_str(), "missing 2D profile"))?;
                    for child in &children[1..] {
                        let clip = child
                            .profile
                            .as_ref()
                            .ok_or_else(|| bad_args(name.as_str(), "missing 2D profile"))?;
                        profile = profile::boolean(op, &profile, clip)?;
                    }
                    if profile.is_empty() {
                        return Err(bad_args(
                            name.as_str(),
                            "2D boolean produced empty geometry",
                        ));
                    }
                    let shape = self.kernel.sketch_profile(&profile::to_regions(&profile))?;
                    return Ok(Node {
                        shape,
                        module,
                        source_range,
                        parameters,
                        transform_parameters: None,
                        world: parent,
                        dimension: Dimension::TwoD,
                        profile: Some(profile),
                        children,
                    });
                }

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
                    profile: None,
                    children,
                })
            }
            "hull" | "minkowski" => {
                args.ensure_empty(name.as_str())?;
                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, parent),
                    None => {
                        let child = single_child_statement(&node)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a child or block"))?;
                        evaluator.statement(child, parent)
                    }
                })?;
                if children.is_empty() {
                    return Err(bad_args(name.as_str(), "expected at least one child"));
                }
                if children
                    .iter()
                    .any(|child| child.dimension != Dimension::ThreeD)
                {
                    return Err(bad_args(name.as_str(), "supports only 3D children"));
                }

                let shape = if name == "hull" {
                    let mut points = Vec::new();
                    for child in &children {
                        let mesh = self.kernel.tessellate(child.shape, self.tolerance)?;
                        points.extend(mesh.positions);
                    }
                    let (points, faces) = hull::convex_hull(&points)
                        .ok_or_else(|| bad_args("hull", "degenerate hull"))?;
                    self.kernel
                        .primitive(Primitive::Polyhedron { points, faces })?
                } else if children.len() == 1 {
                    children[0].shape
                } else {
                    let mut groups = Vec::new();
                    for child in &children {
                        let mesh = self.kernel.tessellate(child.shape, self.tolerance)?;
                        groups.push(mesh.positions);
                    }
                    let points = hull::minkowski_points(&groups)
                        .ok_or_else(|| bad_args("minkowski", "empty Minkowski sum"))?;
                    let (points, faces) = hull::convex_hull(&points)
                        .ok_or_else(|| bad_args("minkowski", "degenerate Minkowski sum"))?;
                    self.kernel
                        .primitive(Primitive::Polyhedron { points, faces })?
                };

                Ok(Node {
                    shape,
                    module,
                    source_range,
                    parameters,
                    transform_parameters: None,
                    world: parent,
                    dimension: Dimension::ThreeD,
                    profile: None,
                    children,
                })
            }
            _ => Err(EvalError::UnsupportedModule(name)),
        }
    }

    fn block(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        self.register_declarations(&node);
        self.apply_assignments(&node)?;
        let mut nodes = Vec::new();
        for statement in node.children() {
            if statement.kind() == SyntaxKind::ASSIGNMENT {
                continue;
            }
            nodes.extend(self.statement(statement, parent)?);
        }
        Ok(nodes)
    }

    fn apply_assignments(&mut self, node: &SyntaxNode) -> Result<(), EvalError> {
        let mut order = Vec::new();
        let mut last = HashMap::new();
        for assignment in node
            .children()
            .filter(|child| child.kind() == SyntaxKind::ASSIGNMENT)
        {
            let name = first_ident(&assignment)
                .ok_or_else(|| EvalError::Parse("missing assignment name".to_owned()))?;
            if !last.contains_key(&name) {
                order.push(name.clone());
            }
            last.insert(name, assignment);
        }
        for name in order {
            let assignment = last
                .remove(&name)
                .expect("assignment recorded in first-occurrence order");
            self.assignment(assignment)?;
        }
        Ok(())
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

    fn centering_offset(&self, translation: [f64; 3]) -> Transform {
        self.placement_offset(translation, None)
    }

    fn rotate_transform(&self, args: &Arguments) -> Result<Transform, EvalError> {
        args.ensure_known("rotate", &["a", "v"])?;
        if args.positional.len() > 2 {
            return Err(bad_args("rotate", "expected an angle and optional axis"));
        }
        let angle = match args.named.get("a").or_else(|| args.positional.first()) {
            None | Some(Value::Undef) => None,
            Some(value) => Some(value),
        };
        let axis = match args.named.get("v").or_else(|| args.positional.get(1)) {
            None | Some(Value::Undef) => None,
            Some(value) => Some(value),
        };
        if let Some(axis) = axis {
            let angle = angle
                .ok_or_else(|| bad_args("rotate", "expected an angle"))?
                .as_number("rotate")?;
            let axis = axis.as_vector3("rotate")?;
            return Transform::from_rotation_axis(axis, angle.to_radians())
                .ok_or_else(|| bad_args("rotate", "axis must be non-zero and finite"));
        }
        match angle {
            Some(Value::Number(angle)) => Ok(Transform::from_rotation_z(angle.to_radians())),
            Some(value) => {
                let [x, y, z] = value.as_vector3("rotate")?;
                Ok(Transform::from_rotation_xyz([
                    x.to_radians(),
                    y.to_radians(),
                    z.to_radians(),
                ]))
            }
            None => Ok(Transform::IDENTITY),
        }
    }

    fn placement_offset(&self, translation: [f64; 3], center: Option<bool>) -> Transform {
        let centered = center.unwrap_or(!self.legacy_centering);
        if centered {
            Transform::IDENTITY
        } else {
            Transform::from_translation(translation)
        }
    }

    fn transform_tree(&mut self, node: &mut Node, transform: Transform) -> Result<(), EvalError> {
        if transform.is_identity() {
            return Ok(());
        }
        node.shape = self.kernel.transform(node.shape, transform)?;
        node.world = transform.compose(node.world);
        if let Some(profile) = &node.profile {
            node.profile = Some(profile::transform(profile, transform));
        }
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
                    if name.starts_with('$') {
                        arguments.special.insert(name, value);
                    } else {
                        arguments.named_order.push(name.clone());
                        arguments.named.insert(name, value);
                    }
                }
                _ => arguments.positional.push(self.element_value(element)?),
            }
        }
        Ok(arguments)
    }

    fn special_number(&self, args: &Arguments, name: &str) -> Result<Option<f64>, EvalError> {
        if let Some(value) = args.special.get(name) {
            return Ok(Some(value.as_number(name)?));
        }
        self.lookup_optional(name)
            .map(|value| value.as_number(name))
            .transpose()
    }

    fn fn_segments(&self, args: &Arguments) -> Result<Option<usize>, EvalError> {
        let Some(value) = self.special_number(args, "$fn")? else {
            return Ok(None);
        };
        if !value.is_finite() {
            return Err(bad_args("$fn", "must be finite"));
        }
        if value <= 0.0 || value > FACETED_MAX as f64 {
            return Ok(None);
        }
        Ok(Some(value.ceil().max(3.0) as usize))
    }

    fn profile_segments(&self, args: &Arguments, radius: f64) -> Result<usize, EvalError> {
        if let Some(value) = self.special_number(args, "$fn")? {
            if !value.is_finite() {
                return Err(bad_args("$fn", "must be finite"));
            }
            if value > 0.0 {
                return Ok((value.ceil().max(3.0) as usize).min(PROFILE_SEGMENTS_MAX));
            }
        }

        let fa = self.special_number(args, "$fa")?;
        let fs = self.special_number(args, "$fs")?;
        if fa.is_none() && fs.is_none() {
            return Ok(profile::CIRCLE_SEGMENTS);
        }

        let fa = fa.map_or(12.0, |value| value.max(0.01));
        let fs = fs.map_or(2.0, |value| value.max(0.01));
        let segments = (360.0 / fa)
            .min(std::f64::consts::TAU * radius / fs)
            .max(5.0)
            .ceil() as usize;
        Ok(segments.clamp(3, PROFILE_SEGMENTS_MAX))
    }

    fn with_special_overrides<T>(
        &mut self,
        args: &Arguments,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        if args.special.is_empty() {
            return f(self);
        }
        let parent = self.current_env();
        let env = Env::child(parent.clone(), parent);
        env.variables.borrow_mut().extend(args.special.clone());
        self.with_env(env, f)
    }

    fn element_value(&mut self, element: SyntaxElement) -> Result<Value, EvalError> {
        match element {
            NodeOrToken::Token(token) => match token.kind() {
                SyntaxKind::NUMBER => Ok(Value::Number(parse_number(token.text())?)),
                SyntaxKind::TRUE => Ok(Value::Boolean(true)),
                SyntaxKind::FALSE => Ok(Value::Boolean(false)),
                SyntaxKind::UNDEF => Ok(Value::Undef),
                SyntaxKind::IDENT => self.lookup(token.text()),
                SyntaxKind::STRING => Ok(Value::String(parse_string_literal(token.text()))),
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
            SyntaxKind::RANGE => self.range_value(node),
            SyntaxKind::BINARY_EXPR => self.binary_expr_value(node),
            SyntaxKind::UNARY_EXPR => self.unary_expr_value(node),
            SyntaxKind::GROUP => self.group_value(node),
            SyntaxKind::CALL_EXPR => self.call_value(node),
            SyntaxKind::CONDITIONAL_EXPR => self.conditional_value(node),
            SyntaxKind::INDEX_EXPR => self.index_value(node),
            SyntaxKind::FOR_EXPR | SyntaxKind::IF_EXPR => self.sequence_value(node),
            SyntaxKind::C_FOR_EXPR => self.c_for_value(node),
            SyntaxKind::LET_EXPR => self.let_value(node),
            SyntaxKind::ECHO_EXPR | SyntaxKind::ASSERT_EXPR => self.prefixed_value(node),
            SyntaxKind::FUNCTION_LITERAL => self.function_literal_value(node),
            SyntaxKind::INVOKE_EXPR => self.invoke_value(node),
            SyntaxKind::DOT_EXPR => self.dot_value(node),
            kind => Err(EvalError::Expression(format!(
                "unexpected node in expression: {kind:?} [{}]",
                node.text().to_string().trim()
            ))),
        }
    }

    fn call_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let name = first_ident(node)
            .ok_or_else(|| EvalError::Parse("missing function name".to_owned()))?;
        let args = match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        self.call_stack.push(name.clone());
        let result = match self.lookup_function(&name) {
            Some(declaration) => self.user_function_call(&name, &declaration, args),
            None => match self.lookup_optional(&name) {
                Some(Value::Function(function)) => self.call_function_value(&function, args),
                _ => self.builtin_function(&name, &args),
            },
        };
        self.call_stack.pop();
        result
    }

    fn function_literal_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let param_list = node
            .children()
            .find(|child| child.kind() == SyntaxKind::PARAM_LIST)
            .ok_or_else(|| EvalError::Expression("function literal needs parameters".to_owned()))?;
        let parameters = declaration_parameters(&param_list);
        let body = function_literal_body(node)?;
        let source = node.text().to_string().trim().to_owned();
        Ok(Value::Function(Rc::new(FunctionValue {
            parameters,
            body,
            source,
            env: self.current_env(),
        })))
    }

    fn call_function_value(
        &mut self,
        function: &FunctionValue,
        args: Arguments,
    ) -> Result<Value, EvalError> {
        for named in args.named.keys() {
            if !function.parameters.iter().any(|(param, _)| param == named) {
                return Err(bad_args(
                    "function",
                    format!("unsupported named argument {named}"),
                ));
            }
        }
        if args.positional.len() > function.parameters.len() {
            return Err(bad_args("function", "too many arguments"));
        }

        let mut bindings = HashMap::new();
        for (index, value) in args.positional.iter().enumerate() {
            bindings.insert(function.parameters[index].0.clone(), value.clone());
        }
        for (param, value) in &args.named {
            bindings.insert(param.clone(), value.clone());
        }

        let call_env = Env::child(function.env.clone(), self.current_env());
        call_env.variables.borrow_mut().extend(args.special.clone());

        self.with_env(call_env.clone(), |evaluator| {
            for (param, default) in &function.parameters {
                let value = match bindings.get(param) {
                    Some(value) => value.clone(),
                    None => match default {
                        Some(expression) => evaluator.element_value(expression.clone())?,
                        None => Value::Undef,
                    },
                };
                call_env.variables.borrow_mut().insert(param.clone(), value);
            }
            evaluator.element_value(function.body.clone())
        })
    }

    fn invoke_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut callee = None;
        let mut arg_list = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::ARG_LIST => {
                    if let NodeOrToken::Node(child) = element {
                        arg_list = Some(child);
                    }
                }
                kind if kind.is_trivia() => {}
                _ if callee.is_none() => callee = Some(element),
                _ => {}
            }
        }
        let callee = self.element_value(
            callee.ok_or_else(|| EvalError::Expression("missing called value".to_owned()))?,
        )?;
        let args = match arg_list {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        match callee {
            Value::Function(function) => self.call_function_value(&function, args),
            _ => Err(bad_args("function", "value is not callable")),
        }
    }

    fn user_function_call(
        &mut self,
        name: &str,
        declaration: &Declaration,
        args: Arguments,
    ) -> Result<Value, EvalError> {
        let declaration_node = &declaration.node;
        let parameters = declaration_node
            .children()
            .find(|child| child.kind() == SyntaxKind::PARAM_LIST)
            .map(|list| declaration_parameters(&list))
            .unwrap_or_default();
        for named in args.named.keys() {
            if !parameters.iter().any(|(param, _)| param == named) {
                return Err(bad_args(
                    name,
                    format!("unsupported named argument {named}"),
                ));
            }
        }
        if args.positional.len() > parameters.len() {
            return Err(bad_args(name, "too many arguments"));
        }

        let mut bindings = HashMap::new();
        for (index, value) in args.positional.iter().enumerate() {
            bindings.insert(parameters[index].0.clone(), value.clone());
        }
        for (param, value) in &args.named {
            bindings.insert(param.clone(), value.clone());
        }

        let body = function_expression(declaration_node)
            .ok_or_else(|| bad_args(name, "missing function body"))?;
        let special = args.special.clone();
        let call_env = Env::child(declaration.env.clone(), self.current_env());

        self.with_env(call_env.clone(), |evaluator| {
            {
                let mut variables = call_env.variables.borrow_mut();
                for (name, value) in &special {
                    variables.insert(name.clone(), value.clone());
                }
            }
            for (param, default) in &parameters {
                let value = match bindings.get(param) {
                    Some(value) => value.clone(),
                    None => match default {
                        Some(expression) => evaluator.element_value(expression.clone())?,
                        None => Value::Undef,
                    },
                };
                call_env.variables.borrow_mut().insert(param.clone(), value);
            }
            evaluator.element_value(body.clone())
        })
    }

    fn builtin_function(&self, name: &str, args: &Arguments) -> Result<Value, EvalError> {
        let allowed = match name {
            "search" => &["num_returns_per_match", "index_col_num"][..],
            "rands" => &["seed_value"][..],
            _ => &[][..],
        };
        args.ensure_known(name, allowed)?;

        match name {
            "abs" | "sign" | "sqrt" | "ln" | "log" | "exp" | "floor" | "ceil" | "round" | "sin"
            | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh"
            | "acosh" | "atanh" => {
                let value = single_number(name, args)?;
                let result = match name {
                    "abs" => value.abs(),
                    "sign" => {
                        if value == 0.0 {
                            0.0
                        } else {
                            value.signum()
                        }
                    }
                    "sqrt" => value.sqrt(),
                    "ln" => value.ln(),
                    "log" => value.log10(),
                    "exp" => value.exp(),
                    "floor" => value.floor(),
                    "ceil" => value.ceil(),
                    "round" => value.round(),
                    "sin" => value.to_radians().sin(),
                    "cos" => value.to_radians().cos(),
                    "tan" => value.to_radians().tan(),
                    "asin" => value.asin().to_degrees(),
                    "acos" => value.acos().to_degrees(),
                    "atan" => value.atan().to_degrees(),
                    "sinh" => value.sinh(),
                    "cosh" => value.cosh(),
                    "tanh" => value.tanh(),
                    "asinh" => value.asinh(),
                    "acosh" => value.acosh(),
                    _ => value.atanh(),
                };
                Ok(Value::Number(result))
            }
            "atan2" => {
                let [y, x] = two_numbers(name, args)?;
                Ok(Value::Number(y.atan2(x).to_degrees()))
            }
            "pow" => {
                let [base, exponent] = two_numbers(name, args)?;
                Ok(Value::Number(base.powf(exponent)))
            }
            "min" | "max" => {
                let values = match number_list(name, args) {
                    Ok(values) => values,
                    Err(_) => return Ok(Value::Undef),
                };
                let result = if name == "min" {
                    values.into_iter().reduce(f64::min)
                } else {
                    values.into_iter().reduce(f64::max)
                };
                Ok(result.map(Value::Number).unwrap_or(Value::Undef))
            }
            "norm" => {
                let Value::List(values) = single_value(name, args)? else {
                    return Err(bad_args(name, "expected a vector"));
                };
                let mut sum = 0.0;
                for value in values {
                    let value = value.as_number(name)?;
                    sum += value * value;
                }
                Ok(Value::Number(sum.sqrt()))
            }
            "cross" => {
                let [left, right] = two_values(name, args)?;
                let values = |value: &Value| -> Result<Vec<f64>, EvalError> {
                    let Value::List(values) = value else {
                        return Err(bad_args(name, "expected two vectors"));
                    };
                    if !matches!(values.len(), 2 | 3) {
                        return Err(bad_args(name, "expected two 2D or 3D vectors"));
                    }
                    values.iter().map(|value| value.as_number(name)).collect()
                };
                let left = values(left)?;
                let right = values(right)?;
                match (left.as_slice(), right.as_slice()) {
                    ([x1, y1], [x2, y2]) => Ok(Value::Number(x1 * y2 - y1 * x2)),
                    ([x1, y1, z1], [x2, y2, z2]) => Ok(Value::List(vec![
                        Value::Number(y1 * z2 - z1 * y2),
                        Value::Number(z1 * x2 - x1 * z2),
                        Value::Number(x1 * y2 - y1 * x2),
                    ])),
                    _ => Err(bad_args(name, "expected two vectors of the same dimension")),
                }
            }
            "len" => match single_value(name, args)? {
                Value::List(values) => Ok(Value::Number(values.len() as f64)),
                Value::String(text) => Ok(Value::Number(text.chars().count() as f64)),
                _ => Err(bad_args(name, "expected a vector or string")),
            },
            "str" => {
                let mut text = String::new();
                for value in &args.positional {
                    text.push_str(&format_value(value));
                }
                Ok(Value::String(text))
            }
            "chr" => {
                let value = single_value(name, args)?;
                let convert = |code: f64| -> Result<char, EvalError> {
                    if !code.is_finite() || code < 0.0 || code.fract() != 0.0 {
                        return Err(bad_args(name, "code point must be a non-negative integer"));
                    }
                    char::from_u32(code as u32)
                        .ok_or_else(|| bad_args(name, "invalid Unicode code point"))
                };
                match value {
                    Value::Number(code) => Ok(Value::String(convert(*code)?.to_string())),
                    Value::List(values) => {
                        let mut text = String::new();
                        for value in values {
                            let Value::Number(code) = value else {
                                return Err(bad_args(name, "code points must be numbers"));
                            };
                            text.push(convert(*code)?);
                        }
                        Ok(Value::String(text))
                    }
                    _ => Err(bad_args(name, "expected a number or list of numbers")),
                }
            }
            "ord" => {
                let text = single_value(name, args)?.as_string(name)?;
                text.chars()
                    .next()
                    .map(|character| Value::Number(character as u32 as f64))
                    .ok_or_else(|| bad_args(name, "expected a non-empty string"))
            }
            "concat" => {
                let mut values = Vec::new();
                for value in &args.positional {
                    match value {
                        Value::List(items) => values.extend(items.iter().cloned()),
                        value => values.push(value.clone()),
                    }
                }
                Ok(Value::List(values))
            }
            "lookup" => {
                let [key, table] = two_values(name, args)?;
                let key = key.as_number(name)?;
                let Value::List(rows) = table else {
                    return Err(bad_args(name, "expected a table of key/value rows"));
                };
                let mut entries = Vec::with_capacity(rows.len());
                for row in rows {
                    let Value::List(values) = row else {
                        return Err(bad_args(name, "expected a table of key/value rows"));
                    };
                    let [stored, value, ..] = values.as_slice() else {
                        return Err(bad_args(name, "expected a table of key/value rows"));
                    };
                    entries.push((stored.as_number(name)?, value.clone()));
                }
                entries.sort_by(|left, right| left.0.total_cmp(&right.0));
                if entries.is_empty() {
                    return Ok(Value::Undef);
                }
                if key <= entries[0].0 {
                    return Ok(entries[0].1.clone());
                }
                let last = entries.len() - 1;
                if key >= entries[last].0 {
                    return Ok(entries[last].1.clone());
                }
                for pair in entries.windows(2) {
                    let (left_key, left_value) = &pair[0];
                    let (right_key, right_value) = &pair[1];
                    if key == *left_key {
                        return Ok(left_value.clone());
                    }
                    if key == *right_key {
                        return Ok(right_value.clone());
                    }
                    if key > *left_key && key < *right_key {
                        let amount = (key - left_key) / (right_key - left_key);
                        return Ok(interpolate_value(left_value, right_value, amount));
                    }
                }
                Ok(Value::Undef)
            }
            "search" => search_value(name, args),
            "rands" => rands_value(name, args),
            "is_num" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Number(_)
            ))),
            "is_nan" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Number(value) if value.is_nan()
            ))),
            "is_bool" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Boolean(_)
            ))),
            "is_list" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::List(_)
            ))),
            "is_string" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::String(_)
            ))),
            "is_undef" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Undef
            ))),
            "is_function" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Function(_)
            ))),
            "version" => {
                args.ensure_empty(name)?;
                Ok(Value::List(vec![Value::Number(2021.0), Value::Number(1.0)]))
            }
            "version_num" => {
                args.ensure_empty(name)?;
                Ok(Value::Number(20_210_100.0))
            }
            _ => Err(EvalError::UnknownFunction(name.to_owned())),
        }
    }

    fn conditional_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut condition = None;
        let mut then_value = None;
        let mut otherwise = None;
        let mut stage = 0;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::QUESTION => stage = 1,
                SyntaxKind::COLON => stage = 2,
                kind if kind.is_trivia() => {}
                _ => {
                    let slot = match stage {
                        0 => &mut condition,
                        1 => &mut then_value,
                        _ => &mut otherwise,
                    };
                    if slot.is_some() {
                        return Err(EvalError::Expression(
                            "unexpected operand in conditional expression".to_owned(),
                        ));
                    }
                    *slot = Some(element);
                }
            }
        }

        let condition = self.element_value(
            condition.ok_or_else(|| EvalError::Expression("missing condition".to_owned()))?,
        )?;
        if condition.as_boolean("?")? {
            self.element_value(
                then_value
                    .ok_or_else(|| EvalError::Expression("missing true branch".to_owned()))?,
            )
        } else {
            self.element_value(
                otherwise
                    .ok_or_else(|| EvalError::Expression("missing false branch".to_owned()))?,
            )
        }
    }

    fn index_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut base = None;
        let mut index = None;
        let mut in_index = false;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_BRACK => in_index = true,
                SyntaxKind::R_BRACK => {}
                kind if kind.is_trivia() => {}
                _ if !in_index => base = Some(element),
                _ => index = Some(element),
            }
        }

        let base = self.element_value(
            base.ok_or_else(|| EvalError::Expression("missing indexed value".to_owned()))?,
        )?;
        let index = self.element_value(
            index.ok_or_else(|| EvalError::Expression("missing index".to_owned()))?,
        )?;
        let Value::Number(index) = index else {
            return Ok(Value::Undef);
        };
        if index < 0.0 || index.fract() != 0.0 {
            return Ok(Value::Undef);
        }
        let index = index as usize;

        match base {
            Value::List(values) => Ok(values.get(index).cloned().unwrap_or(Value::Undef)),
            Value::String(text) => Ok(text
                .chars()
                .nth(index)
                .map(|character| Value::String(character.to_string()))
                .unwrap_or(Value::Undef)),
            _ => Ok(Value::Undef),
        }
    }

    fn sequence_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        Ok(Value::List(
            self.comprehension_values(&NodeOrToken::Node(node.clone()))?,
        ))
    }

    fn c_for_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut stage = 0;
        let mut init = Vec::new();
        let mut condition = None;
        let mut next = Vec::new();
        let mut body = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::SEMICOLON => {
                    stage += 1;
                    continue;
                }
                SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::COMMA => continue,
                kind if kind.is_trivia() => continue,
                _ => {}
            }
            match stage {
                0 => {
                    if let NodeOrToken::Node(child) = element {
                        if child.kind() == SyntaxKind::NAMED_ARG {
                            init.push(child);
                        }
                    }
                }
                1 => condition = Some(element),
                2 => {
                    if element.kind() == SyntaxKind::NAMED_ARG {
                        if let NodeOrToken::Node(child) = element {
                            next.push(child);
                        }
                    } else if body.is_none() {
                        body = Some(element);
                    }
                }
                _ => {}
            }
        }

        let body = body.ok_or_else(|| EvalError::Expression("missing for body".to_owned()))?;
        let mut values = Vec::new();
        self.with_child_scope(|evaluator| {
            for assignment in &init {
                evaluator.c_for_assignment(assignment)?;
            }
            let mut iterations = 0usize;
            loop {
                if let Some(condition) = &condition {
                    if !evaluator
                        .element_value(condition.clone())?
                        .as_boolean("for")?
                    {
                        break;
                    }
                }
                values.extend(evaluator.comprehension_values(&body)?);
                for assignment in &next {
                    evaluator.c_for_assignment(assignment)?;
                }
                iterations += 1;
                if iterations > 100_000 {
                    return Err(EvalError::Expression(
                        "for loop exceeded 100000 iterations".to_owned(),
                    ));
                }
            }
            Ok(())
        })?;
        Ok(Value::List(values))
    }

    fn c_for_assignment(&mut self, assignment: &SyntaxNode) -> Result<(), EvalError> {
        let name = first_ident(assignment)
            .ok_or_else(|| EvalError::Parse("missing assignment name".to_owned()))?;
        let value = self
            .expression_after_eq(assignment)?
            .ok_or_else(|| EvalError::Parse(format!("missing expression for {name}")))?;
        self.current_env()
            .variables
            .borrow_mut()
            .insert(name, value);
        Ok(())
    }

    fn let_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let arg_list = comprehension_arguments(node)?;
        let body = comprehension_body(node)?;
        self.with_let_bindings(&arg_list, |evaluator| evaluator.element_value(body))
    }

    fn comprehension_values(&mut self, element: &SyntaxElement) -> Result<Vec<Value>, EvalError> {
        let NodeOrToken::Node(node) = element else {
            return Ok(vec![self.element_value(element.clone())?]);
        };
        match node.kind() {
            SyntaxKind::FOR_EXPR => {
                let arg_list = comprehension_arguments(node)?;
                let bindings = self.comprehension_bindings(&arg_list)?;
                if bindings.is_empty() {
                    return Err(EvalError::Expression(
                        "list comprehension needs at least one binding".to_owned(),
                    ));
                }
                let body = comprehension_body(node)?;
                let mut values = Vec::new();
                self.for_comprehension_values(&bindings, 0, &body, &mut values)?;
                Ok(values)
            }
            SyntaxKind::IF_EXPR => {
                let arg_list = comprehension_arguments(node)?;
                let args = self.arg_list(arg_list)?;
                if !args.named.is_empty() || args.positional.len() != 1 {
                    return Err(EvalError::Expression(
                        "list comprehension condition needs one argument".to_owned(),
                    ));
                }
                let condition = args.positional[0].as_boolean("if")?;
                let branch = if condition {
                    comprehension_body(node)?
                } else {
                    match else_body(node) {
                        Some(body) => body,
                        None => return Ok(Vec::new()),
                    }
                };
                self.comprehension_values(&branch)
            }
            SyntaxKind::LET_EXPR => {
                let arg_list = comprehension_arguments(node)?;
                let body = comprehension_body(node)?;
                self.with_let_bindings(&arg_list, |evaluator| evaluator.comprehension_values(&body))
            }
            SyntaxKind::GROUP => {
                let inner = group_inner(node)
                    .ok_or_else(|| EvalError::Expression("empty group".to_owned()))?;
                self.comprehension_values(&inner)
            }
            SyntaxKind::EACH_EXPR => {
                let mut body = None;
                let mut seen_keyword = false;
                for element in node.children_with_tokens() {
                    match element.kind() {
                        SyntaxKind::IDENT if !seen_keyword => seen_keyword = true,
                        kind if kind.is_trivia() => {}
                        _ => {
                            body = Some(element);
                            break;
                        }
                    }
                }
                let body =
                    body.ok_or_else(|| EvalError::Expression("each needs a value".to_owned()))?;
                let value = self.element_value(body)?;
                Ok(each_values(value))
            }
            SyntaxKind::C_FOR_EXPR => {
                let value = self.c_for_value(node)?;
                Ok(each_values(value))
            }
            _ => Ok(vec![self.node_value(node)?]),
        }
    }

    fn for_comprehension_values(
        &mut self,
        bindings: &[(String, SyntaxElement)],
        index: usize,
        body: &SyntaxElement,
        values: &mut Vec<Value>,
    ) -> Result<(), EvalError> {
        if index == bindings.len() {
            values.extend(self.comprehension_values(body)?);
            return Ok(());
        }

        let (name, expression) = &bindings[index];
        let value = self.element_value(expression.clone())?;
        let items = loop_values(name, value)?;
        for value in items {
            let env_parent = self.current_env();
            let env = Env::child(env_parent.clone(), env_parent);
            env.variables.borrow_mut().insert(name.clone(), value);
            self.scopes.push(env);
            let result = self.for_comprehension_values(bindings, index + 1, body, values);
            self.scopes.pop();
            result?;
        }
        Ok(())
    }

    fn comprehension_bindings(
        &mut self,
        arg_list: &SyntaxNode,
    ) -> Result<Vec<(String, SyntaxElement)>, EvalError> {
        let mut bindings = Vec::new();
        for element in arg_list.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::COMMA => {}
                kind if kind.is_trivia() => {}
                SyntaxKind::NAMED_ARG => {
                    let Some(child) = element.into_node() else {
                        continue;
                    };
                    let name = first_ident(&child)
                        .ok_or_else(|| EvalError::Parse("missing binding name".to_owned()))?;
                    let value = expression_element_after_eq(&child)?.ok_or_else(|| {
                        EvalError::Parse(format!("missing value for binding {name}"))
                    })?;
                    bindings.push((name, value));
                }
                _ => {
                    return Err(EvalError::Expression(
                        "list comprehension bindings must be named".to_owned(),
                    ));
                }
            }
        }
        Ok(bindings)
    }

    fn with_let_bindings<T>(
        &mut self,
        arg_list: &SyntaxNode,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        self.with_child_scope(|evaluator| {
            for element in arg_list.children_with_tokens() {
                match element.kind() {
                    SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::COMMA => {}
                    kind if kind.is_trivia() => {}
                    SyntaxKind::NAMED_ARG => {
                        let Some(child) = element.into_node() else {
                            continue;
                        };
                        let name = first_ident(&child)
                            .ok_or_else(|| EvalError::Parse("missing binding name".to_owned()))?;
                        let value = evaluator.expression_after_eq(&child)?.ok_or_else(|| {
                            EvalError::Parse(format!("missing value for binding {name}"))
                        })?;
                        evaluator
                            .current_env()
                            .variables
                            .borrow_mut()
                            .insert(name, value);
                    }
                    _ => {
                        return Err(EvalError::Expression(
                            "let bindings must be named".to_owned(),
                        ));
                    }
                }
            }
            f(evaluator)
        })
    }

    fn binary_expr_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut op = None;
        let mut left = None;
        let mut right = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::STAR
                | SyntaxKind::SLASH
                | SyntaxKind::PERCENT
                | SyntaxKind::CARET
                | SyntaxKind::EQEQ
                | SyntaxKind::NE
                | SyntaxKind::LT
                | SyntaxKind::LE
                | SyntaxKind::GT
                | SyntaxKind::GE
                | SyntaxKind::AND_AND
                | SyntaxKind::OR_OR => op = Some(element.kind()),
                kind if kind.is_trivia() => {}
                _ if left.is_none() => left = Some(element),
                _ if right.is_none() => right = Some(element),
                _ => {
                    return Err(EvalError::Expression(
                        "too many operands in binary expression".to_owned(),
                    ));
                }
            }
        }

        let op = op.ok_or_else(|| EvalError::Expression("missing operator".to_owned()))?;
        let left = self.element_value(
            left.ok_or_else(|| EvalError::Expression("missing left operand".to_owned()))?,
        )?;
        if op == SyntaxKind::AND_AND && !left.as_boolean("&&")? {
            return Ok(Value::Boolean(false));
        }
        if op == SyntaxKind::OR_OR && left.as_boolean("||")? {
            return Ok(Value::Boolean(true));
        }
        let right = self.element_value(
            right.ok_or_else(|| EvalError::Expression("missing right operand".to_owned()))?,
        )?;
        binary_value(op, left, right)
    }

    fn unary_expr_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut operand = None;
        let mut op = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::BANG => {
                    op = Some(element.kind())
                }
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
        let inner =
            group_inner(node).ok_or_else(|| EvalError::Expression("empty group".to_owned()))?;
        self.element_value(inner)
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
        if let Some(range) = node
            .children()
            .find(|child| child.kind() == SyntaxKind::RANGE)
        {
            return self.range_value(&range);
        }

        let mut values = Vec::new();
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_BRACK | SyntaxKind::R_BRACK | SyntaxKind::COMMA => {}
                kind if kind.is_trivia() => {}
                _ => {
                    if let NodeOrToken::Node(child) = &element {
                        if is_comprehension(child) {
                            values.extend(self.comprehension_values(&element)?);
                            continue;
                        }
                    }
                    values.push(self.element_value(element)?);
                }
            }
        }
        Ok(Value::List(values))
    }

    fn range_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        let mut numbers = Vec::new();
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::COLON => {}
                kind if kind.is_trivia() => {}
                _ => numbers.push(self.element_value(element)?.as_number("range")?),
            }
        }

        let (start, step, end) = match numbers.as_slice() {
            [start, end] => (*start, 1.0, *end),
            [start, step, end] => (*start, *step, *end),
            _ => {
                return Err(EvalError::Expression(
                    "range needs two or three numbers".to_owned(),
                ));
            }
        };
        if !start.is_finite() || !step.is_finite() || !end.is_finite() {
            return Err(EvalError::Expression(
                "range values must be finite".to_owned(),
            ));
        }
        if step == 0.0 {
            return Err(EvalError::Expression(
                "range step must not be zero".to_owned(),
            ));
        }

        let mut values = Vec::new();
        let mut value = start;
        if step > 0.0 {
            while value <= end + 1.0e-9 {
                values.push(value);
                value += step;
            }
        } else {
            while value >= end - 1.0e-9 {
                values.push(value);
                value += step;
            }
        }
        Ok(Value::List(values.into_iter().map(Value::Number).collect()))
    }

    fn lookup(&self, name: &str) -> Result<Value, EvalError> {
        Ok(self.lookup_optional(name).unwrap_or(Value::Undef))
    }

    fn lookup_optional(&self, name: &str) -> Option<Value> {
        let dynamic = name.starts_with('$');
        let mut env = Some(self.current_env());
        while let Some(current) = env {
            if let Some(value) = current.variables.borrow().get(name) {
                return Some(value.clone());
            }
            env = if dynamic {
                current.dynamic_parent.clone()
            } else {
                current.lexical_parent.clone()
            };
        }
        None
    }
}

fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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

fn loop_values(_name: &str, value: Value) -> Result<Vec<Value>, EvalError> {
    Ok(match value {
        Value::List(values) => values,
        Value::String(text) => text
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect(),
        Value::Undef => Vec::new(),
        value => vec![value],
    })
}

fn child_index(value: f64) -> Result<usize, EvalError> {
    if value < 0.0 || value.fract() != 0.0 {
        return Err(bad_args("children", "index must be a non-negative integer"));
    }
    Ok(value as usize)
}

fn each_values(value: Value) -> Vec<Value> {
    match value {
        Value::List(values) => values,
        Value::String(text) => text
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect(),
        value => vec![value],
    }
}

fn echo_message(args: &Arguments) -> String {
    let mut parts = args
        .positional
        .iter()
        .map(format_echo_value)
        .collect::<Vec<_>>();
    for name in &args.named_order {
        if let Some(value) = args.named.get(name) {
            parts.push(format!("{name} = {}", format_echo_value(value)));
        }
    }
    format!("ECHO: {}", parts.join(", "))
}

impl<K: Kernel> Evaluator<'_, K> {
    fn assertion_error(&self, node: &SyntaxNode, error: EvalError) -> EvalError {
        match error {
            EvalError::Assertion(message) => {
                let stack = self.call_stack.join(" > ");
                EvalError::Assertion(format!(
                    "{message} [{}] (at {stack})",
                    node.text().to_string().trim()
                ))
            }
            error => error,
        }
    }
}

fn cylinder_frustum(
    radius1: f64,
    radius2: f64,
    height: f64,
    segments: usize,
) -> (Vec<[f64; 3]>, Vec<Vec<usize>>) {
    let mut points = Vec::new();
    let bottom_is_apex = radius1 == 0.0;
    if !bottom_is_apex {
        for index in 0..segments {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            points.push([radius1 * angle.cos(), radius1 * angle.sin(), 0.0]);
        }
    } else {
        points.push([0.0, 0.0, 0.0]);
    }
    let top_start = points.len();
    let top_is_apex = radius2 == 0.0;
    if !top_is_apex {
        for index in 0..segments {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            points.push([radius2 * angle.cos(), radius2 * angle.sin(), height]);
        }
    } else {
        points.push([0.0, 0.0, height]);
    }

    let mut faces = Vec::new();
    if !bottom_is_apex {
        faces.push((0..segments).rev().collect());
    }
    if !top_is_apex {
        faces.push((top_start..top_start + segments).collect());
    }
    for index in 0..segments {
        let next = (index + 1) % segments;
        let bottom = index;
        let bottom_next = next;
        let top = top_start + index;
        let top_next = top_start + next;
        match (bottom_is_apex, top_is_apex) {
            (false, false) => {
                faces.push(vec![bottom, bottom_next, top_next]);
                faces.push(vec![bottom, top_next, top]);
            }
            (true, false) => faces.push(vec![0, top_next, top]),
            (false, true) => faces.push(vec![bottom, bottom_next, top_start]),
            (true, true) => unreachable!("both cylinder radii cannot be zero"),
        }
    }
    (points, faces)
}

fn check_assertion(args: &Arguments) -> Result<(), EvalError> {
    let condition = args
        .positional
        .first()
        .ok_or_else(|| bad_args("assert", "expected a condition"))?;
    if condition.as_boolean("assert")? {
        return Ok(());
    }
    let message = if args.positional.len() > 1 {
        args.positional[1..]
            .iter()
            .map(format_echo_value)
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "assertion failed".to_owned()
    };
    Err(EvalError::Assertion(message))
}

fn function_expression(node: &SyntaxNode) -> Option<SyntaxElement> {
    let mut after_eq = false;
    for element in node.children_with_tokens() {
        match element.kind() {
            SyntaxKind::EQ => after_eq = true,
            kind if kind.is_trivia() => {}
            SyntaxKind::SEMICOLON => break,
            _ if after_eq => return Some(element),
            _ => {}
        }
    }
    None
}

fn function_literal_body(node: &SyntaxNode) -> Result<SyntaxElement, EvalError> {
    let mut after_params = false;
    for element in node.children_with_tokens() {
        if element.kind() == SyntaxKind::PARAM_LIST {
            after_params = true;
            continue;
        }
        if !after_params || element.kind().is_trivia() {
            continue;
        }
        return Ok(element);
    }
    Err(EvalError::Expression(
        "missing function literal body".to_owned(),
    ))
}

fn parse_error(path: &Path, parsed: &brepscad_syntax::Parse) -> EvalError {
    let messages = parsed
        .errors()
        .iter()
        .map(|error| format!("{} at offset {}", error.message, error.offset))
        .collect::<Vec<_>>()
        .join("; ");
    EvalError::Parse(format!("{}: {messages}", path.display()))
}

fn include_path(node: &SyntaxNode) -> Option<(String, bool)> {
    let mut in_angle = false;
    let mut angle = String::new();
    for element in node.children_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        match token.kind() {
            SyntaxKind::STRING => return Some((parse_string_literal(token.text()), false)),
            SyntaxKind::LT => in_angle = true,
            SyntaxKind::GT if in_angle => return Some((angle, true)),
            kind if kind.is_trivia() => {}
            _ if in_angle => angle.push_str(token.text()),
            _ => {}
        }
    }
    None
}

fn parse_string_literal(text: &str) -> String {
    let inner = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text);
    let mut result = String::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('x') => match hex_escape(&mut characters, 2) {
                Some('\0') => result.push(' '),
                Some(character) => result.push(character),
                None => {
                    result.push('\\');
                    result.push('x');
                }
            },
            Some('u') => match hex_escape(&mut characters, 4) {
                Some('\0') => result.push(' '),
                Some(character) => result.push(character),
                None => {
                    result.push('\\');
                    result.push('u');
                }
            },
            Some('U') => match hex_escape(&mut characters, 8) {
                Some('\0') => result.push(' '),
                Some(character) => result.push(character),
                None => {
                    result.push('\\');
                    result.push('U');
                }
            },
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

fn hex_escape(characters: &mut std::str::Chars<'_>, digits: usize) -> Option<char> {
    let mut probe = characters.clone();
    let mut value = 0u32;
    for _ in 0..digits {
        value = value * 16 + probe.next()?.to_digit(16)?;
    }
    *characters = probe;
    char::from_u32(value)
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Number(value) => format_number(*value),
        Value::Boolean(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_echo_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Function(function) => function.source.clone(),
        Value::Undef => "undef".to_owned(),
    }
}

fn format_echo_value(value: &Value) -> String {
    match value {
        Value::String(text) => {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        Value::Number(value) => format_number(*value),
        Value::Boolean(value) => value.to_string(),
        Value::Undef => "undef".to_owned(),
        Value::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_echo_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Function(function) => function.source.clone(),
    }
}

fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1.0e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

fn group_inner(node: &SyntaxNode) -> Option<SyntaxElement> {
    for element in node.children_with_tokens() {
        match element.kind() {
            SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
            kind if kind.is_trivia() => {}
            _ => return Some(element),
        }
    }
    None
}

fn is_comprehension(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::FOR_EXPR
        | SyntaxKind::IF_EXPR
        | SyntaxKind::EACH_EXPR
        | SyntaxKind::C_FOR_EXPR => true,
        SyntaxKind::LET_EXPR => comprehension_body(node)
            .ok()
            .and_then(|body| body.into_node())
            .is_some_and(|body| is_comprehension(&body)),
        SyntaxKind::GROUP => group_inner(node)
            .and_then(|inner| inner.into_node())
            .is_some_and(|inner| is_comprehension(&inner)),
        _ => false,
    }
}

fn expression_element_after_eq(node: &SyntaxNode) -> Result<Option<SyntaxElement>, EvalError> {
    let mut after_eq = false;
    for element in node.children_with_tokens() {
        match element.kind() {
            SyntaxKind::EQ => after_eq = true,
            kind if kind.is_trivia() => {}
            _ if after_eq => return Ok(Some(element)),
            _ => {}
        }
    }
    Ok(None)
}

fn comprehension_arguments(node: &SyntaxNode) -> Result<SyntaxNode, EvalError> {
    node.children()
        .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        .ok_or_else(|| EvalError::Expression("missing comprehension arguments".to_owned()))
}

fn comprehension_body(node: &SyntaxNode) -> Result<SyntaxElement, EvalError> {
    let mut after_args = false;
    for element in node.children_with_tokens() {
        if element.kind() == SyntaxKind::ARG_LIST {
            after_args = true;
            continue;
        }
        if !after_args || element.kind().is_trivia() || element.kind() == SyntaxKind::ELSE {
            continue;
        }
        return Ok(element);
    }
    Err(EvalError::Expression(
        "missing comprehension body".to_owned(),
    ))
}

fn else_body(node: &SyntaxNode) -> Option<SyntaxElement> {
    let else_node = node
        .children()
        .find(|child| child.kind() == SyntaxKind::ELSE)?;
    else_node
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .nth(1)
}

fn single_value<'a>(name: &str, args: &'a Arguments) -> Result<&'a Value, EvalError> {
    if args.positional.len() == 1 {
        Ok(&args.positional[0])
    } else {
        Err(bad_args(name, "expected one argument"))
    }
}

fn single_number(name: &str, args: &Arguments) -> Result<f64, EvalError> {
    single_value(name, args)?.as_number(name)
}

fn two_values<'a>(name: &str, args: &'a Arguments) -> Result<[&'a Value; 2], EvalError> {
    match args.positional.as_slice() {
        [first, second] => Ok([first, second]),
        _ => Err(bad_args(name, "expected two arguments")),
    }
}

fn two_numbers(name: &str, args: &Arguments) -> Result<[f64; 2], EvalError> {
    let [first, second] = two_values(name, args)?;
    Ok([first.as_number(name)?, second.as_number(name)?])
}

fn number_list(name: &str, args: &Arguments) -> Result<Vec<f64>, EvalError> {
    if let [Value::List(values)] = args.positional.as_slice() {
        return values.iter().map(|value| value.as_number(name)).collect();
    }
    args.positional
        .iter()
        .map(|value| value.as_number(name))
        .collect()
}

fn search_value(name: &str, args: &Arguments) -> Result<Value, EvalError> {
    let (target, list) = match args.positional.as_slice() {
        [target, list, ..] => (target, list),
        _ => return Err(bad_args(name, "expected a match and a list")),
    };
    let num_returns = search_number(
        name,
        args.named
            .get("num_returns_per_match")
            .or_else(|| args.positional.get(2)),
        1.0,
    )?;
    let num_returns = if num_returns <= 0.0 {
        usize::MAX
    } else {
        num_returns as usize
    };
    let index_col = if let Some(value) = args.named.get("index_col_num") {
        match value {
            Value::Undef => None,
            value => Some(value.as_number(name)?),
        }
    } else if let Some(value) = args.positional.get(3) {
        match value {
            Value::Undef => None,
            value => Some(value.as_number(name)?),
        }
    } else {
        Some(0.0)
    };
    if let Some(index_col) = index_col {
        if index_col < 0.0 || index_col.fract() != 0.0 {
            return Err(bad_args(
                name,
                "index column must be a non-negative integer",
            ));
        }
    }
    let index_col = index_col.map(|value| value as usize);

    let matches = match target {
        Value::List(values) => values.iter().collect::<Vec<_>>(),
        target => vec![target],
    };
    let multiple = matches!(target, Value::List(_));

    let mut results = Vec::new();
    for target in matches {
        let mut indices = Vec::new();
        match list {
            Value::String(text) => {
                if let Value::String(needle) = target {
                    let needle = needle.chars().collect::<Vec<_>>();
                    if needle.len() == 1 {
                        for (index, character) in text.chars().enumerate() {
                            if character == needle[0] {
                                indices.push(Value::Number(index as f64));
                                if indices.len() >= num_returns {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Value::List(values) => {
                for (index, item) in values.iter().enumerate() {
                    let candidate = match (item, index_col) {
                        (Value::List(row), Some(index_col)) => {
                            row.get(index_col).unwrap_or(&Value::Undef)
                        }
                        (item, _) => item,
                    };
                    if candidate == target {
                        indices.push(Value::Number(index as f64));
                        if indices.len() >= num_returns {
                            break;
                        }
                    }
                }
            }
            _ => return Err(bad_args(name, "expected a vector or string")),
        }
        results.push(Value::List(indices));
    }

    if multiple {
        Ok(Value::List(results))
    } else {
        Ok(results
            .into_iter()
            .next()
            .unwrap_or(Value::List(Vec::new())))
    }
}

fn search_number(name: &str, value: Option<&Value>, default: f64) -> Result<f64, EvalError> {
    match value {
        None | Some(Value::Undef) => Ok(default),
        Some(value) => value.as_number(name),
    }
}

fn rands_value(name: &str, args: &Arguments) -> Result<Value, EvalError> {
    let (minimum, maximum, count) = match args.positional.as_slice() {
        [minimum, maximum, count] => (minimum, maximum, count),
        [minimum, maximum, count, _seed] => (minimum, maximum, count),
        _ => {
            return Err(bad_args(name, "expected minimum, maximum, and count"));
        }
    };
    let minimum = random_bounds(name, minimum)?;
    let maximum = random_bounds(name, maximum)?;
    if minimum.len() != maximum.len() {
        return Err(bad_args(
            name,
            "minimum and maximum must have equal lengths",
        ));
    }
    let count = count.as_number(name)?;
    if count < 0.0 || count.fract() != 0.0 {
        return Err(bad_args(name, "count must be a non-negative integer"));
    }
    let count = count as usize;
    let seed = match args
        .named
        .get("seed_value")
        .or_else(|| args.positional.get(3))
    {
        None | Some(Value::Undef) => 0.0,
        Some(value) => value.as_number(name)?,
    };

    let mut random = Random(seed.to_bits() ^ 0x9E37_79B9_7F4A_7C15);
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        if minimum.len() == 1 {
            values.push(Value::Number(random.between(minimum[0], maximum[0])));
        } else {
            values.push(Value::List(
                minimum
                    .iter()
                    .zip(&maximum)
                    .map(|(minimum, maximum)| Value::Number(random.between(*minimum, *maximum)))
                    .collect(),
            ));
        }
    }
    Ok(Value::List(values))
}

fn interpolate_value(left: &Value, right: &Value, amount: f64) -> Value {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            Value::Number(left + (right - left) * amount)
        }
        (Value::List(left), Value::List(right)) if left.len() == right.len() => Value::List(
            left.iter()
                .zip(right)
                .map(|(left, right)| interpolate_value(left, right, amount))
                .collect(),
        ),
        _ => Value::Undef,
    }
}

fn random_bounds(name: &str, value: &Value) -> Result<Vec<f64>, EvalError> {
    match value {
        Value::Number(value) => Ok(vec![*value]),
        Value::List(values) => values.iter().map(|value| value.as_number(name)).collect(),
        _ => Err(bad_args(name, "expected a number or a vector of numbers")),
    }
}

struct Random(u64);

impl Random {
    fn between(&mut self, minimum: f64, maximum: f64) -> f64 {
        minimum + (maximum - minimum) * self.next_f64()
    }

    fn next_f64(&mut self) -> f64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        let bits = value.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (bits >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn module_name(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .nth(1)
        .map(|token| token.text().to_owned())
}

fn declaration_parameters(node: &SyntaxNode) -> Vec<(String, Option<SyntaxElement>)> {
    let mut parameters = Vec::new();
    for param in node
        .children()
        .filter(|child| child.kind() == SyntaxKind::PARAM)
    {
        let mut name = None;
        let mut default = None;
        let mut after_eq = false;
        for element in param.children_with_tokens() {
            match element.kind() {
                SyntaxKind::IDENT if name.is_none() => {
                    name = element.into_token().map(|token| token.text().to_owned());
                }
                SyntaxKind::EQ => after_eq = true,
                kind if kind.is_trivia() => {}
                _ if after_eq && default.is_none() => default = Some(element),
                _ => {}
            }
        }
        if let Some(name) = name {
            parameters.push((name, default));
        }
    }
    parameters
}

fn single_child_statement(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().find(|child| {
        matches!(
            child.kind(),
            SyntaxKind::MODULE_CALL | SyntaxKind::INCLUDE | SyntaxKind::USE
        )
    })
}

fn call_children(node: &SyntaxNode) -> Vec<SyntaxNode> {
    match node
        .children()
        .find(|child| child.kind() == SyntaxKind::BLOCK)
    {
        Some(block) => block.children().collect(),
        None => single_child_statement(node).into_iter().collect(),
    }
}

fn merge_parameters(node: &mut Node, parameters: &[Parameter]) {
    for parameter in parameters {
        if node
            .parameters
            .iter()
            .all(|existing| existing.range != parameter.range)
        {
            node.parameters.push(parameter.clone());
        }
    }
    node.parameters
        .sort_by_key(|parameter| parameter.range.start);
    for child in &mut node.children {
        merge_parameters(child, parameters);
    }
}
