//! Evaluator connecting the source tree to a kernel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Sketch, Transform};
use brepscad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, parse};

use crate::cache::MeshCache;
use crate::model::{EvaluatedShape, Evaluation, Parameter};
use crate::parameters::{collect_parameters, first_ident, parse_number, text_range_to_range};
use crate::parts::{Dimension, Node, collect_parts};
use crate::profile;
use crate::value::{Value, binary_value, unary_value};
use crate::{EvalError, bad_args};

const SINGULAR_DETERMINANT: f64 = 1.0e-12;
const FACETED_MAX: usize = 32;
const PROFILE_SEGMENTS_MAX: usize = 256;

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
#[derive(Debug, Clone, Copy)]
pub struct EvaluationOptions {
    pub tolerance: f64,
    /// Place cubes/squares at the origin, cylinder bases at `z = 0`, and
    /// linear extrusions from `z = 0`, matching OpenSCAD defaults.
    pub legacy_centering: bool,
}

impl Default for EvaluationOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.01,
            legacy_centering: false,
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
    evaluate_with_options(
        source,
        kernel,
        EvaluationOptions {
            tolerance,
            legacy_centering: false,
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
        scopes: vec![Scope::default()],
        children_stack: Vec::new(),
        tolerance,
        legacy_centering: options.legacy_centering,
        source_dir,
        include_stack,
        messages: Vec::new(),
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
    scopes: Vec<Scope>,
    children_stack: Vec<ChildrenFrame>,
    tolerance: f64,
    legacy_centering: bool,
    source_dir: Option<PathBuf>,
    include_stack: Vec<PathBuf>,
    messages: Vec<String>,
}

#[derive(Debug, Default)]
struct Scope {
    variables: HashMap<String, Value>,
    assignments: HashMap<String, SyntaxNode>,
    modules: HashMap<String, SyntaxNode>,
    functions: HashMap<String, SyntaxNode>,
}

#[derive(Debug)]
struct ChildrenFrame {
    statements: Vec<SyntaxNode>,
    caller_depth: usize,
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
        let scope = self.scopes.last_mut().expect("root scope");
        scope.assignments.insert(name.clone(), node);
        scope.variables.insert(name, value);
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
            _ => match self.lookup_module(&name) {
                Some(declaration) => self.user_module_call(node, parent, &name, declaration),
                None => self.module_call_one(node, parent).map(|node| vec![node]),
            },
        }
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
        check_assertion(&args)?;
        self.call_children(node, parent)
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
            check_assertion(&args)?;
        }

        let mut body = None;
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::IDENT | SyntaxKind::ARG_LIST => {}
                kind if kind.is_trivia() => {}
                _ => {
                    body = Some(element);
                    break;
                }
            }
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
            Value::Vector(values) => Ok(values
                .get(index)
                .copied()
                .map(Value::Number)
                .unwrap_or(Value::Undef)),
            Value::Matrix(rows) => Ok(rows
                .get(index)
                .cloned()
                .map(Value::Vector)
                .unwrap_or(Value::Undef)),
            _ => Ok(Value::Undef),
        }
    }

    fn user_module_call(
        &mut self,
        node: SyntaxNode,
        parent: Transform,
        name: &str,
        declaration: SyntaxNode,
    ) -> Result<Vec<Node>, EvalError> {
        let args = match node
            .children()
            .find(|child| child.kind() == SyntaxKind::ARG_LIST)
        {
            Some(arg_list) => self.arg_list(arg_list)?,
            None => Arguments::default(),
        };
        let parameters = declaration
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
        let caller_depth = self.scopes.len();
        let special = args.special.clone();

        let mut nodes = self.with_scope(Scope::default(), |evaluator| {
            {
                let scope = evaluator.scopes.last_mut().expect("module scope");
                for (name, value) in &special {
                    scope.variables.insert(name.clone(), value.clone());
                }
                scope.variables.insert(
                    "$children".to_owned(),
                    Value::Number(statements.len() as f64),
                );
            }

            for (param, default) in &parameters {
                let value = match bindings.get(param) {
                    Some(value) => value.clone(),
                    None => match default {
                        Some(expression) => evaluator.element_value(expression.clone())?,
                        None => {
                            return Err(bad_args(name, format!("missing argument {param}")));
                        }
                    },
                };
                evaluator
                    .scopes
                    .last_mut()
                    .expect("module scope")
                    .variables
                    .insert(param.clone(), value);
            }

            evaluator.children_stack.push(ChildrenFrame {
                statements,
                caller_depth,
                special,
            });
            let body = declaration
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
        let (statements, caller_depth, special) = match self.children_stack.last() {
            Some(frame) => (
                frame.statements.clone(),
                frame.caller_depth,
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
            Some(Value::Vector(indices)) => {
                let mut selected = Vec::new();
                for index in indices {
                    let index = child_index(*index)?;
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

        let saved = self.scopes.split_off(caller_depth);
        let pushed_scope = !special.is_empty();
        if pushed_scope {
            self.scopes.push(Scope {
                variables: special,
                ..Scope::default()
            });
        }
        let result = (|| {
            let mut nodes = Vec::new();
            for statement in selected {
                nodes.extend(self.statement(statement, parent)?);
            }
            Ok(nodes)
        })();
        if pushed_scope {
            self.scopes.pop();
        }
        self.scopes.extend(saved);
        result
    }

    fn module_body(&mut self, body: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        if body.kind() == SyntaxKind::BLOCK {
            self.block(body, parent)
        } else {
            self.statement(body, parent)
        }
    }

    fn with_scope<T>(
        &mut self,
        scope: Scope,
        f: impl FnOnce(&mut Self) -> Result<T, EvalError>,
    ) -> Result<T, EvalError> {
        self.scopes.push(scope);
        let result = f(self);
        self.scopes.pop();
        result
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
        let scope = self.scopes.last_mut().expect("root scope");
        for (name, declaration) in modules {
            scope.modules.insert(name, declaration);
        }
        for (name, declaration) in functions {
            scope.functions.insert(name, declaration);
        }
    }

    fn lookup_module(&self, name: &str) -> Option<SyntaxNode> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.modules.get(name).cloned())
    }

    fn lookup_function(&self, name: &str) -> Option<SyntaxNode> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.functions.get(name).cloned())
    }

    fn evaluate_source_tree(
        &mut self,
        root: &SyntaxNode,
        parent: Transform,
    ) -> Result<Vec<Node>, EvalError> {
        self.register_declarations(root);
        let mut nodes = Vec::new();
        for statement in root.children() {
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
        let request = include_path(&node)
            .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
        let path = self.resolve_source(&request)?;
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
                evaluator.register_declarations_deep(&root)?;
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
            let request = include_path(&statement)
                .ok_or_else(|| EvalError::Source("missing include path".to_owned()))?;
            let path = self.resolve_source(&request)?;
            let source = std::fs::read_to_string(&path).map_err(|error| {
                EvalError::Source(format!("failed to read {}: {error}", path.display()))
            })?;
            let parsed = parse(&source);
            if !parsed.errors().is_empty() {
                return Err(parse_error(&path, &parsed));
            }
            let child = parsed.syntax();
            self.with_source_file(path, |evaluator| {
                evaluator.register_declarations_deep(&child)
            })?;
        }
        Ok(())
    }

    fn resolve_source(&self, request: &str) -> Result<PathBuf, EvalError> {
        let path = Path::new(request);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.source_dir
                .as_ref()
                .ok_or_else(|| {
                    EvalError::Source(format!("include \"{request}\" requires a source file path"))
                })?
                .join(path)
        };
        Ok(std::fs::canonicalize(&resolved).unwrap_or(resolved))
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
        for scope in &self.scopes {
            assignments.extend(
                scope
                    .assignments
                    .iter()
                    .map(|(name, node)| (name.clone(), node.clone())),
            );
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
        let args = self.arg_list(arg_list)?;
        if !args.positional.is_empty() {
            return Err(bad_args("for", "expected named loop bindings"));
        }
        if args.named_order.is_empty() {
            return Err(bad_args("for", "expected at least one loop binding"));
        }

        let mut bindings = Vec::with_capacity(args.named_order.len());
        for name in &args.named_order {
            let value = args
                .named
                .get(name)
                .cloned()
                .ok_or_else(|| bad_args("for", format!("missing value for {name}")))?;
            bindings.push((name.clone(), loop_values(name, value)?));
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
        bindings: &[(String, Vec<Value>)],
        index: usize,
        body: &SyntaxNode,
        parent: Transform,
        nodes: &mut Vec<Node>,
    ) -> Result<(), EvalError> {
        if index == bindings.len() {
            nodes.extend(self.eval_control_body(body.clone(), parent)?);
            return Ok(());
        }

        let (name, values) = &bindings[index];
        let mut outcome = Ok(());
        for value in values {
            let mut scope = Scope::default();
            scope.variables.insert(name.clone(), value.clone());
            self.scopes.push(scope);
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
                args.ensure_known("cube", &["size"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("cube", "expected one argument"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("cube", "expected size"))?
                    .as_size("cube")?;
                let local = self.centering_offset([size[0] / 2.0, size[1] / 2.0, size[2] / 2.0]);
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
                let local = self.centering_offset([0.0, 0.0, height / 2.0]);
                let world = parent.compose(local);
                let shape = match self.fn_segments(&args)? {
                    Some(segments) => {
                        let profile = profile::circle_with_segments(radius, segments);
                        let sketch = self.kernel.sketch_profile(&profile::to_regions(&profile))?;
                        self.kernel.extrude(sketch, height)?
                    }
                    None => self
                        .kernel
                        .primitive(Primitive::Cylinder { radius, height })?,
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
                args.ensure_known("polyhedron", &["points", "faces", "convexity"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("polyhedron", "expected points and faces"));
                }
                let points = args
                    .argument("points", 0)
                    .ok_or_else(|| bad_args("polyhedron", "expected points"))?
                    .as_points3("polyhedron")?;
                let faces = args
                    .argument("faces", 1)
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
                args.ensure_known("square", &["size"])?;
                if args.positional.len() > 1 {
                    return Err(bad_args("square", "expected one size"));
                }
                let size = args
                    .argument("size", 0)
                    .ok_or_else(|| bad_args("square", "expected size"))?
                    .as_size2("square")?;
                if size.iter().any(|value| *value <= 0.0) {
                    return Err(bad_args("square", "dimensions must be positive"));
                }
                let local = self.centering_offset([size[0] / 2.0, size[1] / 2.0, 0.0]);
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
                args.ensure_known("polygon", &["points"])?;
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
                let profile = profile::polygon(&points);
                let profile = profile::transform(&profile, parent);
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
                args.ensure_known("linear_extrude", &["height", "center"])?;
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
                if children.len() != 1 {
                    return Err(bad_args("linear_extrude", "expected exactly one 2D child"));
                }
                if children[0].dimension != Dimension::TwoD {
                    return Err(bad_args("linear_extrude", "expected a 2D child"));
                }

                let shape = self.kernel.extrude(children[0].shape, height)?;
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
                args.ensure_known("rotate_extrude", &["angle", "start"])?;
                if args.positional.len() > 2 {
                    return Err(bad_args("rotate_extrude", "expected an angle and start"));
                }
                let angle = match args.argument("angle", 0) {
                    Some(value) => value.as_number("rotate_extrude")?,
                    None => 360.0,
                };
                let start = match args.argument("start", 1) {
                    Some(value) => value.as_number("rotate_extrude")?,
                    None => 0.0,
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
                let children = self.with_special_overrides(&args, |evaluator| match block {
                    Some(block) => evaluator.block(block, parent),
                    None => {
                        let child = single_child_statement(&node)
                            .ok_or_else(|| bad_args(name.as_str(), "expected a block or child"))?;
                        evaluator.statement(child, parent)
                    }
                })?;
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
            _ => Err(EvalError::UnsupportedModule(name)),
        }
    }

    fn block(&mut self, node: SyntaxNode, parent: Transform) -> Result<Vec<Node>, EvalError> {
        self.register_declarations(&node);
        let mut nodes = Vec::new();
        for statement in node.children() {
            nodes.extend(self.statement(statement, parent)?);
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

    fn centering_offset(&self, translation: [f64; 3]) -> Transform {
        if self.legacy_centering {
            Transform::from_translation(translation)
        } else {
            Transform::IDENTITY
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
        self.with_scope(
            Scope {
                variables: args.special.clone(),
                ..Scope::default()
            },
            f,
        )
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
            SyntaxKind::DOT_EXPR => self.dot_value(node),
            kind => Err(EvalError::Expression(format!(
                "unexpected node in expression: {kind:?}"
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
        match self.lookup_function(&name) {
            Some(declaration) => self.user_function_call(&name, &declaration, args),
            None => self.builtin_function(&name, &args),
        }
    }

    fn user_function_call(
        &mut self,
        name: &str,
        declaration: &SyntaxNode,
        args: Arguments,
    ) -> Result<Value, EvalError> {
        let parameters = declaration
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

        let body = function_expression(declaration)
            .ok_or_else(|| bad_args(name, "missing function body"))?;
        let special = args.special.clone();

        self.with_scope(Scope::default(), |evaluator| {
            {
                let scope = evaluator.scopes.last_mut().expect("function scope");
                for (name, value) in &special {
                    scope.variables.insert(name.clone(), value.clone());
                }
            }
            for (param, default) in &parameters {
                let value = match bindings.get(param) {
                    Some(value) => value.clone(),
                    None => match default {
                        Some(expression) => evaluator.element_value(expression.clone())?,
                        None => {
                            return Err(bad_args(name, format!("missing argument {param}")));
                        }
                    },
                };
                evaluator
                    .scopes
                    .last_mut()
                    .expect("function scope")
                    .variables
                    .insert(param.clone(), value);
            }
            evaluator.element_value(body.clone())
        })
    }

    fn builtin_function(&self, name: &str, args: &Arguments) -> Result<Value, EvalError> {
        args.ensure_known(name, &[])?;

        match name {
            "abs" | "sign" | "sqrt" | "ln" | "log" | "exp" | "floor" | "ceil" | "round" | "sin"
            | "cos" | "tan" | "asin" | "acos" | "atan" => {
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
                    _ => value.atan().to_degrees(),
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
                let values = number_list(name, args)?;
                let result = if name == "min" {
                    values.into_iter().reduce(f64::min)
                } else {
                    values.into_iter().reduce(f64::max)
                };
                result
                    .map(Value::Number)
                    .ok_or_else(|| bad_args(name, "expected at least one number"))
            }
            "norm" => {
                let Value::Vector(values) = single_value(name, args)? else {
                    return Err(bad_args(name, "expected a vector"));
                };
                Ok(Value::Number(
                    values.iter().map(|value| value * value).sum::<f64>().sqrt(),
                ))
            }
            "cross" => {
                let [left, right] = two_values(name, args)?;
                let (Value::Vector(left), Value::Vector(right)) = (left, right) else {
                    return Err(bad_args(name, "expected two vectors"));
                };
                if left.len() != 3 || right.len() != 3 {
                    return Err(bad_args(name, "expected two 3-element vectors"));
                }
                Ok(Value::Vector(vec![
                    left[1] * right[2] - left[2] * right[1],
                    left[2] * right[0] - left[0] * right[2],
                    left[0] * right[1] - left[1] * right[0],
                ]))
            }
            "len" => match single_value(name, args)? {
                Value::Vector(values) => Ok(Value::Number(values.len() as f64)),
                Value::Matrix(rows) => Ok(Value::Number(rows.len() as f64)),
                Value::String(text) => Ok(Value::Number(text.chars().count() as f64)),
                _ => Err(bad_args(name, "expected a vector, matrix, or string")),
            },
            "str" => {
                let mut text = String::new();
                for value in &args.positional {
                    text.push_str(&format_value(value));
                }
                Ok(Value::String(text))
            }
            "chr" => {
                let value = single_number(name, args)?;
                if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
                    return Err(bad_args(name, "code point must be a non-negative integer"));
                }
                char::from_u32(value as u32)
                    .map(|character| Value::String(character.to_string()))
                    .ok_or_else(|| bad_args(name, "invalid Unicode code point"))
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
                    let Value::Vector(items) = value else {
                        return Err(bad_args(name, "expected vectors"));
                    };
                    values.extend(items);
                }
                Ok(Value::Vector(values))
            }
            "is_num" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Number(_)
            ))),
            "is_bool" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Boolean(_)
            ))),
            "is_list" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Vector(_) | Value::Matrix(_)
            ))),
            "is_string" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::String(_)
            ))),
            "is_undef" => Ok(Value::Boolean(matches!(
                single_value(name, args)?,
                Value::Undef
            ))),
            "version" => {
                args.ensure_empty(name)?;
                Ok(Value::Vector(vec![2021.0, 1.0]))
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
        let index = index.as_number("index")?;
        if index < 0.0 || index.fract() != 0.0 {
            return Err(EvalError::Expression(
                "index must be a non-negative integer".to_owned(),
            ));
        }
        let index = index as usize;

        match base {
            Value::Vector(values) => values
                .get(index)
                .copied()
                .map(Value::Number)
                .ok_or_else(|| EvalError::Expression(format!("index {index} is out of range"))),
            Value::Matrix(rows) => rows
                .get(index)
                .cloned()
                .map(Value::Vector)
                .ok_or_else(|| EvalError::Expression(format!("index {index} is out of range"))),
            Value::String(text) => text
                .chars()
                .nth(index)
                .map(|character| Value::String(character.to_string()))
                .ok_or_else(|| EvalError::Expression(format!("index {index} is out of range"))),
            _ => Err(EvalError::Expression(
                "only vectors, matrices, and strings can be indexed".to_owned(),
            )),
        }
    }

    fn sequence_value(&mut self, node: &SyntaxNode) -> Result<Value, EvalError> {
        values_to_value(self.comprehension_values(&NodeOrToken::Node(node.clone()))?)
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
        self.with_scope(Scope::default(), |evaluator| {
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
        values_to_value(values)
    }

    fn c_for_assignment(&mut self, assignment: &SyntaxNode) -> Result<(), EvalError> {
        let name = first_ident(assignment)
            .ok_or_else(|| EvalError::Parse("missing assignment name".to_owned()))?;
        let value = self
            .expression_after_eq(assignment)?
            .ok_or_else(|| EvalError::Parse(format!("missing expression for {name}")))?;
        self.scopes
            .last_mut()
            .expect("for scope")
            .variables
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
            SyntaxKind::EACH_EXPR => {
                let mut body = None;
                for element in node.children_with_tokens() {
                    match element.kind() {
                        SyntaxKind::IDENT => {}
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
        bindings: &[(String, Vec<Value>)],
        index: usize,
        body: &SyntaxElement,
        values: &mut Vec<Value>,
    ) -> Result<(), EvalError> {
        if index == bindings.len() {
            values.extend(self.comprehension_values(body)?);
            return Ok(());
        }

        let (name, items) = &bindings[index];
        for value in items {
            let mut scope = Scope::default();
            scope.variables.insert(name.clone(), value.clone());
            self.scopes.push(scope);
            let result = self.for_comprehension_values(bindings, index + 1, body, values);
            self.scopes.pop();
            result?;
        }
        Ok(())
    }

    fn comprehension_bindings(
        &mut self,
        arg_list: &SyntaxNode,
    ) -> Result<Vec<(String, Vec<Value>)>, EvalError> {
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
                    let value = self.expression_after_eq(&child)?.ok_or_else(|| {
                        EvalError::Parse(format!("missing value for binding {name}"))
                    })?;
                    bindings.push((name.clone(), loop_values(&name, value)?));
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
        self.with_scope(Scope::default(), |evaluator| {
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
                            .scopes
                            .last_mut()
                            .expect("let scope")
                            .variables
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
        if let Some(range) = node
            .children()
            .find(|child| child.kind() == SyntaxKind::RANGE)
        {
            return self.range_value(&range);
        }

        let mut numbers = Vec::new();
        let mut rows = Vec::new();
        for element in node.children_with_tokens() {
            match element.kind() {
                SyntaxKind::L_BRACK | SyntaxKind::R_BRACK | SyntaxKind::COMMA => {}
                kind if kind.is_trivia() => {}
                _ => {
                    if let NodeOrToken::Node(child) = &element {
                        if matches!(
                            child.kind(),
                            SyntaxKind::FOR_EXPR
                                | SyntaxKind::IF_EXPR
                                | SyntaxKind::EACH_EXPR
                                | SyntaxKind::C_FOR_EXPR
                        ) {
                            for value in self.comprehension_values(&element)? {
                                push_sequence_value(value, &mut numbers, &mut rows)?;
                            }
                            continue;
                        }
                    }
                    push_sequence_value(self.element_value(element)?, &mut numbers, &mut rows)?;
                }
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
        Ok(Value::Vector(values))
    }

    fn lookup(&self, name: &str) -> Result<Value, EvalError> {
        self.lookup_optional(name)
            .ok_or_else(|| EvalError::UnknownIdentifier(name.to_owned()))
    }

    fn lookup_optional(&self, name: &str) -> Option<Value> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.variables.get(name).cloned())
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

fn loop_values(name: &str, value: Value) -> Result<Vec<Value>, EvalError> {
    match value {
        Value::Vector(values) => Ok(values.into_iter().map(Value::Number).collect()),
        Value::Matrix(rows) => Ok(rows.into_iter().map(Value::Vector).collect()),
        _ => Err(bad_args(
            "for",
            format!("loop variable {name} must be a vector or range"),
        )),
    }
}

fn child_index(value: f64) -> Result<usize, EvalError> {
    if value < 0.0 || value.fract() != 0.0 {
        return Err(bad_args("children", "index must be a non-negative integer"));
    }
    Ok(value as usize)
}

fn each_values(value: Value) -> Vec<Value> {
    match value {
        Value::Vector(values) => values.into_iter().map(Value::Number).collect(),
        Value::Matrix(rows) => rows.into_iter().map(Value::Vector).collect(),
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

fn parse_error(path: &Path, parsed: &brepscad_syntax::Parse) -> EvalError {
    let messages = parsed
        .errors()
        .iter()
        .map(|error| format!("{} at offset {}", error.message, error.offset))
        .collect::<Vec<_>>()
        .join("; ");
    EvalError::Parse(format!("{}: {messages}", path.display()))
}

fn include_path(node: &SyntaxNode) -> Option<String> {
    let mut in_angle = false;
    let mut angle = String::new();
    for element in node.children_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        match token.kind() {
            SyntaxKind::STRING => return Some(parse_string_literal(token.text())),
            SyntaxKind::LT => in_angle = true,
            SyntaxKind::GT if in_angle => return Some(angle),
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
        Value::Undef => "undef".to_owned(),
        Value::Vector(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| format_number(*value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Matrix(rows) => format!(
            "[{}]",
            rows.iter()
                .map(|row| format!(
                    "[{}]",
                    row.iter()
                        .map(|value| format_number(*value))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
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
        Value::Vector(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| format_echo_value(&Value::Number(*value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Matrix(rows) => format!(
            "[{}]",
            rows.iter()
                .map(|row| format!(
                    "[{}]",
                    row.iter()
                        .map(|value| format_echo_value(&Value::Number(*value)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1.0e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
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

fn push_sequence_value(
    value: Value,
    numbers: &mut Vec<f64>,
    rows: &mut Vec<Vec<f64>>,
) -> Result<(), EvalError> {
    match value {
        Value::Number(value) => numbers.push(value),
        Value::Vector(values) => rows.push(values),
        Value::String(_) => {
            return Err(EvalError::Expression(
                "vectors cannot contain strings".to_owned(),
            ));
        }
        Value::Boolean(_) => {
            return Err(EvalError::Expression(
                "vectors cannot contain booleans".to_owned(),
            ));
        }
        Value::Matrix(_) => {
            return Err(EvalError::Expression(
                "vectors support at most two levels".to_owned(),
            ));
        }
        Value::Undef => {
            return Err(EvalError::Expression(
                "vectors cannot contain undef".to_owned(),
            ));
        }
    }
    Ok(())
}

fn values_to_value(values: Vec<Value>) -> Result<Value, EvalError> {
    let mut numbers = Vec::new();
    let mut rows = Vec::new();
    for value in values {
        push_sequence_value(value, &mut numbers, &mut rows)?;
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
    if let [Value::Vector(values)] = args.positional.as_slice() {
        return Ok(values.clone());
    }
    args.positional
        .iter()
        .map(|value| value.as_number(name))
        .collect()
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
