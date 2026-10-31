//! Persistent lexical environments for evaluator scopes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use brepscad_syntax::SyntaxNode;

use crate::value::Value;

/// A module or function declaration together with the environment it was
/// defined in.
#[derive(Clone)]
pub(crate) struct Declaration {
    pub(crate) node: SyntaxNode,
    pub(crate) env: Rc<Env>,
}

impl fmt::Debug for Declaration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Declaration")
            .field("kind", &self.node.kind())
            .finish()
    }
}

/// A persistent scope.
///
/// Normal variables and declarations are resolved through `lexical_parent`;
/// `$` variables are resolved through `dynamic_parent` to match OpenSCAD's
/// dynamic special variables.
pub(crate) struct Env {
    pub(crate) variables: RefCell<HashMap<String, Value>>,
    pub(crate) assignments: RefCell<HashMap<String, SyntaxNode>>,
    pub(crate) modules: RefCell<HashMap<String, Declaration>>,
    pub(crate) functions: RefCell<HashMap<String, Declaration>>,
    pub(crate) lexical_parent: Option<Rc<Env>>,
    pub(crate) dynamic_parent: Option<Rc<Env>>,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            variables: RefCell::new(HashMap::new()),
            assignments: RefCell::new(HashMap::new()),
            modules: RefCell::new(HashMap::new()),
            functions: RefCell::new(HashMap::new()),
            lexical_parent: None,
            dynamic_parent: None,
        }
    }
}

impl fmt::Debug for Env {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do not recurse through parents: environments can contain function
        // values whose captured environment points back to this one.
        formatter
            .debug_struct("Env")
            .field(
                "variables",
                &self.variables.borrow().keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Env {
    pub(crate) fn root() -> Rc<Self> {
        Rc::new(Self::default())
    }

    pub(crate) fn child(lexical_parent: Rc<Env>, dynamic_parent: Rc<Env>) -> Rc<Self> {
        Rc::new(Self {
            lexical_parent: Some(lexical_parent),
            dynamic_parent: Some(dynamic_parent),
            ..Self::default()
        })
    }
}
