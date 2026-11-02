//! Shared metadata for the built-in module and function names.
//!
//! The evaluator does not depend on this table yet; it is the single source of
//! truth for the editor and the language server instead of maintaining two
//! hand-written name lists.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Module,
    Function,
}

#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    pub name: &'static str,
    pub kind: BuiltinKind,
    pub signature: &'static str,
    pub documentation: &'static str,
}

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "cube",
        kind: BuiltinKind::Module,
        signature: "cube(size, center = false)",
        documentation: "Creates an axis-aligned cube or rectangular block.",
    },
    Builtin {
        name: "cylinder",
        kind: BuiltinKind::Module,
        signature: "cylinder(h, r, r1, r2, center = false)",
        documentation: "Creates a cylinder or truncated cone.",
    },
    Builtin {
        name: "sphere",
        kind: BuiltinKind::Module,
        signature: "sphere(r)",
        documentation: "Creates a sphere.",
    },
    Builtin {
        name: "polyhedron",
        kind: BuiltinKind::Module,
        signature: "polyhedron(points, faces)",
        documentation: "Creates a polyhedron from points and face indices.",
    },
    Builtin {
        name: "square",
        kind: BuiltinKind::Module,
        signature: "square(size, center = false)",
        documentation: "Creates a rectangle or square in the XY plane.",
    },
    Builtin {
        name: "circle",
        kind: BuiltinKind::Module,
        signature: "circle(r)",
        documentation: "Creates a circle in the XY plane.",
    },
    Builtin {
        name: "polygon",
        kind: BuiltinKind::Module,
        signature: "polygon(points)",
        documentation: "Creates a polygon from a list of points.",
    },
    Builtin {
        name: "offset",
        kind: BuiltinKind::Module,
        signature: "offset(r | delta, ...)",
        documentation: "Offsets a 2D shape outwards or inwards.",
    },
    Builtin {
        name: "linear_extrude",
        kind: BuiltinKind::Module,
        signature: "linear_extrude(height, center = false, ...)",
        documentation: "Extrudes a 2D shape along the Z axis.",
    },
    Builtin {
        name: "rotate_extrude",
        kind: BuiltinKind::Module,
        signature: "rotate_extrude(angle = 360, ...)",
        documentation: "Revolves a 2D shape around the Z axis.",
    },
    Builtin {
        name: "resize",
        kind: BuiltinKind::Module,
        signature: "resize(newsize)",
        documentation: "Scales a shape to the requested bounding-box size.",
    },
    Builtin {
        name: "translate",
        kind: BuiltinKind::Module,
        signature: "translate(v)",
        documentation: "Moves its children by a translation vector.",
    },
    Builtin {
        name: "rotate",
        kind: BuiltinKind::Module,
        signature: "rotate(a)",
        documentation: "Rotates its children by an angle or Euler vector.",
    },
    Builtin {
        name: "scale",
        kind: BuiltinKind::Module,
        signature: "scale(v)",
        documentation: "Scales its children by a vector or uniform factor.",
    },
    Builtin {
        name: "multmatrix",
        kind: BuiltinKind::Module,
        signature: "multmatrix(m)",
        documentation: "Applies a 4x4 transformation matrix to its children.",
    },
    Builtin {
        name: "mirror",
        kind: BuiltinKind::Module,
        signature: "mirror(v)",
        documentation: "Mirrors its children across a plane normal.",
    },
    Builtin {
        name: "difference",
        kind: BuiltinKind::Module,
        signature: "difference()",
        documentation: "Subtracts all later children from the first child.",
    },
    Builtin {
        name: "union",
        kind: BuiltinKind::Module,
        signature: "union()",
        documentation: "Combines its children into one shape.",
    },
    Builtin {
        name: "intersection",
        kind: BuiltinKind::Module,
        signature: "intersection()",
        documentation: "Keeps the common volume of all children.",
    },
    Builtin {
        name: "hull",
        kind: BuiltinKind::Module,
        signature: "hull()",
        documentation: "Creates the convex hull of its children.",
    },
    Builtin {
        name: "minkowski",
        kind: BuiltinKind::Module,
        signature: "minkowski()",
        documentation: "Creates the Minkowski sum of its children.",
    },
    Builtin {
        name: "color",
        kind: BuiltinKind::Module,
        signature: "color(c)",
        documentation: "Applies a display color to its children.",
    },
    Builtin {
        name: "render",
        kind: BuiltinKind::Module,
        signature: "render()",
        documentation: "Renders its children without applying a color.",
    },
    Builtin {
        name: "children",
        kind: BuiltinKind::Module,
        signature: "children()",
        documentation: "Passes through the current module's children.",
    },
    Builtin {
        name: "let",
        kind: BuiltinKind::Module,
        signature: "let(assignments)",
        documentation: "Evaluates its children with local assignments.",
    },
    Builtin {
        name: "echo",
        kind: BuiltinKind::Module,
        signature: "echo(values...)",
        documentation: "Prints values to the evaluator messages.",
    },
    Builtin {
        name: "assert",
        kind: BuiltinKind::Module,
        signature: "assert(condition, message)",
        documentation: "Fails evaluation when the condition is false.",
    },
    Builtin {
        name: "for",
        kind: BuiltinKind::Module,
        signature: "for(variable = range)",
        documentation: "Repeats its children for each value in a range or list.",
    },
    Builtin {
        name: "if",
        kind: BuiltinKind::Module,
        signature: "if(condition)",
        documentation: "Evaluates its children only when the condition is true.",
    },
    Builtin {
        name: "abs",
        kind: BuiltinKind::Function,
        signature: "abs(x)",
        documentation: "Absolute value.",
    },
    Builtin {
        name: "sign",
        kind: BuiltinKind::Function,
        signature: "sign(x)",
        documentation: "Sign of a number: -1, 0 or 1.",
    },
    Builtin {
        name: "sqrt",
        kind: BuiltinKind::Function,
        signature: "sqrt(x)",
        documentation: "Square root.",
    },
    Builtin {
        name: "ln",
        kind: BuiltinKind::Function,
        signature: "ln(x)",
        documentation: "Natural logarithm.",
    },
    Builtin {
        name: "log",
        kind: BuiltinKind::Function,
        signature: "log(x)",
        documentation: "Base-10 logarithm.",
    },
    Builtin {
        name: "exp",
        kind: BuiltinKind::Function,
        signature: "exp(x)",
        documentation: "e raised to the given power.",
    },
    Builtin {
        name: "floor",
        kind: BuiltinKind::Function,
        signature: "floor(x)",
        documentation: "Rounds down to an integer.",
    },
    Builtin {
        name: "ceil",
        kind: BuiltinKind::Function,
        signature: "ceil(x)",
        documentation: "Rounds up to an integer.",
    },
    Builtin {
        name: "round",
        kind: BuiltinKind::Function,
        signature: "round(x)",
        documentation: "Rounds to the nearest integer.",
    },
    Builtin {
        name: "sin",
        kind: BuiltinKind::Function,
        signature: "sin(angle)",
        documentation: "Sine of an angle in degrees.",
    },
    Builtin {
        name: "cos",
        kind: BuiltinKind::Function,
        signature: "cos(angle)",
        documentation: "Cosine of an angle in degrees.",
    },
    Builtin {
        name: "tan",
        kind: BuiltinKind::Function,
        signature: "tan(angle)",
        documentation: "Tangent of an angle in degrees.",
    },
    Builtin {
        name: "asin",
        kind: BuiltinKind::Function,
        signature: "asin(x)",
        documentation: "Arcsine in degrees.",
    },
    Builtin {
        name: "acos",
        kind: BuiltinKind::Function,
        signature: "acos(x)",
        documentation: "Arccosine in degrees.",
    },
    Builtin {
        name: "atan",
        kind: BuiltinKind::Function,
        signature: "atan(x)",
        documentation: "Arctangent in degrees.",
    },
    Builtin {
        name: "sinh",
        kind: BuiltinKind::Function,
        signature: "sinh(x)",
        documentation: "Hyperbolic sine.",
    },
    Builtin {
        name: "cosh",
        kind: BuiltinKind::Function,
        signature: "cosh(x)",
        documentation: "Hyperbolic cosine.",
    },
    Builtin {
        name: "tanh",
        kind: BuiltinKind::Function,
        signature: "tanh(x)",
        documentation: "Hyperbolic tangent.",
    },
    Builtin {
        name: "asinh",
        kind: BuiltinKind::Function,
        signature: "asinh(x)",
        documentation: "Inverse hyperbolic sine.",
    },
    Builtin {
        name: "acosh",
        kind: BuiltinKind::Function,
        signature: "acosh(x)",
        documentation: "Inverse hyperbolic cosine.",
    },
    Builtin {
        name: "atanh",
        kind: BuiltinKind::Function,
        signature: "atanh(x)",
        documentation: "Inverse hyperbolic tangent.",
    },
    Builtin {
        name: "pow",
        kind: BuiltinKind::Function,
        signature: "pow(base, exponent)",
        documentation: "Raises a number to a power.",
    },
    Builtin {
        name: "min",
        kind: BuiltinKind::Function,
        signature: "min(values...)",
        documentation: "Smallest value.",
    },
    Builtin {
        name: "max",
        kind: BuiltinKind::Function,
        signature: "max(values...)",
        documentation: "Largest value.",
    },
    Builtin {
        name: "norm",
        kind: BuiltinKind::Function,
        signature: "norm(vector)",
        documentation: "Euclidean length of a vector.",
    },
    Builtin {
        name: "cross",
        kind: BuiltinKind::Function,
        signature: "cross(a, b)",
        documentation: "Cross product of two 3D vectors.",
    },
    Builtin {
        name: "len",
        kind: BuiltinKind::Function,
        signature: "len(value)",
        documentation: "Length of a string or list.",
    },
    Builtin {
        name: "str",
        kind: BuiltinKind::Function,
        signature: "str(values...)",
        documentation: "Concatenates values as text.",
    },
    Builtin {
        name: "chr",
        kind: BuiltinKind::Function,
        signature: "chr(value)",
        documentation: "Converts a Unicode code point to a string.",
    },
    Builtin {
        name: "ord",
        kind: BuiltinKind::Function,
        signature: "ord(string)",
        documentation: "First Unicode code point of a string.",
    },
    Builtin {
        name: "concat",
        kind: BuiltinKind::Function,
        signature: "concat(values...)",
        documentation: "Concatenates lists.",
    },
    Builtin {
        name: "lookup",
        kind: BuiltinKind::Function,
        signature: "lookup(key, table)",
        documentation: "Looks up a value in a sorted key/value table.",
    },
    Builtin {
        name: "search",
        kind: BuiltinKind::Function,
        signature: "search(match, string, num_returns_per_match, index_col_num)",
        documentation: "Searches a string or list for matches.",
    },
    Builtin {
        name: "rands",
        kind: BuiltinKind::Function,
        signature: "rands(min, max, count, seed_value)",
        documentation: "Generates a list of random numbers.",
    },
    Builtin {
        name: "is_num",
        kind: BuiltinKind::Function,
        signature: "is_num(value)",
        documentation: "Tests whether a value is a number.",
    },
    Builtin {
        name: "is_nan",
        kind: BuiltinKind::Function,
        signature: "is_nan(value)",
        documentation: "Tests whether a number is NaN.",
    },
    Builtin {
        name: "is_bool",
        kind: BuiltinKind::Function,
        signature: "is_bool(value)",
        documentation: "Tests whether a value is a boolean.",
    },
    Builtin {
        name: "is_list",
        kind: BuiltinKind::Function,
        signature: "is_list(value)",
        documentation: "Tests whether a value is a list.",
    },
    Builtin {
        name: "is_string",
        kind: BuiltinKind::Function,
        signature: "is_string(value)",
        documentation: "Tests whether a value is a string.",
    },
    Builtin {
        name: "is_undef",
        kind: BuiltinKind::Function,
        signature: "is_undef(value)",
        documentation: "Tests whether a value is undefined.",
    },
    Builtin {
        name: "is_function",
        kind: BuiltinKind::Function,
        signature: "is_function(value)",
        documentation: "Tests whether a value is a function.",
    },
    Builtin {
        name: "version",
        kind: BuiltinKind::Function,
        signature: "version()",
        documentation: "Returns the compatible OpenSCAD version.",
    },
    Builtin {
        name: "version_num",
        kind: BuiltinKind::Function,
        signature: "version_num()",
        documentation: "Returns the compatible OpenSCAD version number.",
    },
];

pub const SPECIAL_VARIABLES: &[(&str, &str)] = &[
    ("$fn", "Fragment count override."),
    ("$fa", "Minimum fragment angle in degrees."),
    ("$fs", "Minimum fragment size."),
    ("$t", "Animation time."),
    ("$children", "Number of children of the current module."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = BUILTINS.iter().map(|builtin| builtin.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILTINS.len());
    }

    #[test]
    fn lookup_finds_modules_and_functions() {
        assert_eq!(lookup("cube").unwrap().kind, BuiltinKind::Module);
        assert_eq!(lookup("sin").unwrap().kind, BuiltinKind::Function);
        assert!(lookup("not_a_builtin").is_none());
    }
}
