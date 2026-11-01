//! Expression values and arithmetic.

use std::rc::Rc;

use brepscad_kernel_api::Transform;
use brepscad_syntax::{SyntaxElement, SyntaxKind};

use crate::scope::Env;
use crate::{EvalError, bad_args};

#[derive(Debug, Clone)]
pub(crate) struct FunctionValue {
    pub(crate) parameters: Vec<(String, Option<SyntaxElement>)>,
    pub(crate) body: SyntaxElement,
    pub(crate) source: String,
    pub(crate) env: Rc<Env>,
}

impl PartialEq for FunctionValue {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters
            && self.body == other.body
            && self.source == other.source
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Number(f64),
    Boolean(bool),
    String(String),
    List(Vec<Value>),
    Function(Rc<FunctionValue>),
    Undef,
}

impl Value {
    pub(crate) fn as_number(&self, module: &str) -> Result<f64, EvalError> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Boolean(_)
            | Self::String(_)
            | Self::List(_)
            | Self::Function(_)
            | Self::Undef => Err(bad_args(module, "expected a number")),
        }
    }

    pub(crate) fn as_boolean(&self, _module: &str) -> Result<bool, EvalError> {
        match self {
            Self::Boolean(value) => Ok(*value),
            Self::Number(value) => Ok(*value != 0.0),
            Self::String(value) => Ok(!value.is_empty()),
            Self::List(values) => Ok(!values.is_empty()),
            Self::Function(_) => Ok(true),
            Self::Undef => Ok(false),
        }
    }

    pub(crate) fn as_string(&self, module: &str) -> Result<&str, EvalError> {
        match self {
            Self::String(value) => Ok(value),
            _ => Err(bad_args(module, "expected a string")),
        }
    }

    pub(crate) fn as_size(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 3]),
            Self::List(values) => match values.as_slice() {
                [x, y, z] => Ok([
                    x.as_number(module)?,
                    y.as_number(module)?,
                    z.as_number(module)?,
                ]),
                _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
            },
            _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
        }
    }

    pub(crate) fn as_size2(&self, module: &str) -> Result<[f64; 2], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 2]),
            Self::List(values) => match values.as_slice() {
                [x, y] => Ok([x.as_number(module)?, y.as_number(module)?]),
                _ => Err(bad_args(module, "expected a 2-element vector or a scalar")),
            },
            _ => Err(bad_args(module, "expected a 2-element vector or a scalar")),
        }
    }

    pub(crate) fn as_polygon_points(&self, module: &str) -> Result<Vec<[f64; 2]>, EvalError> {
        let Self::List(rows) = self else {
            return Err(bad_args(module, "expected a list of 2D points"));
        };
        rows.iter()
            .map(|row| match row {
                Self::List(values) => match values.as_slice() {
                    [x, y] => Ok([x.as_number(module)?, y.as_number(module)?]),
                    _ => Err(bad_args(module, "each point must have two coordinates")),
                },
                _ => Err(bad_args(module, "each point must have two coordinates")),
            })
            .collect()
    }

    pub(crate) fn as_points3(&self, module: &str) -> Result<Vec<[f64; 3]>, EvalError> {
        let Self::List(rows) = self else {
            return Err(bad_args(module, "expected a list of 3D points"));
        };
        rows.iter()
            .map(|row| match row {
                Self::List(values) => match values.as_slice() {
                    [x, y, z] => Ok([
                        x.as_number(module)?,
                        y.as_number(module)?,
                        z.as_number(module)?,
                    ]),
                    _ => Err(bad_args(module, "each point must have three coordinates")),
                },
                _ => Err(bad_args(module, "each point must have three coordinates")),
            })
            .collect()
    }

    pub(crate) fn as_faces(
        &self,
        module: &str,
        point_count: usize,
    ) -> Result<Vec<Vec<usize>>, EvalError> {
        let Self::List(rows) = self else {
            return Err(bad_args(module, "expected a list of faces"));
        };
        rows.iter()
            .map(|row| {
                let Self::List(values) = row else {
                    return Err(bad_args(module, "expected a list of faces"));
                };
                if values.len() < 3 {
                    return Err(bad_args(module, "each face needs at least three vertices"));
                }
                values
                    .iter()
                    .map(|value| {
                        let value = value.as_number(module)?;
                        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
                            return Err(bad_args(
                                module,
                                "face indices must be non-negative integers",
                            ));
                        }
                        let index = value as usize;
                        if index >= point_count {
                            return Err(bad_args(module, "face index out of range"));
                        }
                        Ok(index)
                    })
                    .collect()
            })
            .collect()
    }

    pub(crate) fn as_vector3(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::List(values) => match values.as_slice() {
                [x, y, z] => Ok([
                    x.as_number(module)?,
                    y.as_number(module)?,
                    z.as_number(module)?,
                ]),
                _ => Err(bad_args(module, "expected a 3-element vector")),
            },
            _ => Err(bad_args(module, "expected a 3-element vector")),
        }
    }

    pub(crate) fn as_scale(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 3]),
            Self::List(values) => match values.as_slice() {
                [x, y, z] => Ok([
                    x.as_number(module)?,
                    y.as_number(module)?,
                    z.as_number(module)?,
                ]),
                _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
            },
            _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
        }
    }

    pub(crate) fn as_matrix(&self, module: &str) -> Result<Transform, EvalError> {
        let Self::List(rows) = self else {
            return Err(bad_args(module, "expected a 3x3, 3x4, 4x3, or 4x4 matrix"));
        };
        if !(3..=4).contains(&rows.len()) {
            return Err(bad_args(module, "expected a 3x3, 3x4, 4x3, or 4x4 matrix"));
        }

        let mut matrix = [[0.0; 4]; 4];
        for (index, row) in matrix.iter_mut().enumerate() {
            row[index] = 1.0;
        }
        for (row, values) in rows.iter().take(3).enumerate() {
            let Self::List(values) = values else {
                return Err(bad_args(module, "expected a 3x3, 3x4, 4x3, or 4x4 matrix"));
            };
            if !(3..=4).contains(&values.len()) {
                return Err(bad_args(module, "expected a 3x3, 3x4, 4x3, or 4x4 matrix"));
            }
            for (column, value) in values.iter().enumerate() {
                matrix[row][column] = value.as_number(module)?;
            }
        }
        matrix[3] = [0.0, 0.0, 0.0, 1.0];
        Ok(Transform::from_rows(matrix))
    }
}

fn number_op(op: SyntaxKind, left: f64, right: f64) -> Result<f64, EvalError> {
    match op {
        SyntaxKind::PLUS => Ok(left + right),
        SyntaxKind::MINUS => Ok(left - right),
        SyntaxKind::STAR => Ok(left * right),
        SyntaxKind::CARET => Ok(left.powf(right)),
        SyntaxKind::SLASH => Ok(left / right),
        SyntaxKind::PERCENT => Ok(left % right),
        _ => Err(EvalError::Expression(format!(
            "unsupported operator {op:?}"
        ))),
    }
}

fn numeric_list(value: &Value) -> Option<Vec<f64>> {
    let Value::List(values) = value else {
        return None;
    };
    values
        .iter()
        .map(|value| match value {
            Value::Number(value) => Some(*value),
            _ => None,
        })
        .collect()
}

fn numeric_matrix(value: &Value) -> Option<Vec<Vec<f64>>> {
    let Value::List(rows) = value else {
        return None;
    };
    rows.iter().map(numeric_list).collect()
}

fn scale_list(value: Value, scalar: f64, op: SyntaxKind) -> Result<Value, EvalError> {
    match value {
        Value::Number(value) => Ok(Value::Number(number_op(op, value, scalar)?)),
        Value::List(values) => Ok(Value::List(
            values
                .into_iter()
                .map(|value| scale_list(value, scalar, op))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Value::Undef => Ok(Value::Undef),
        _ => Ok(Value::Undef),
    }
}

fn elementwise(op: SyntaxKind, left: Value, right: Value) -> Result<Value, EvalError> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            Ok(Value::Number(number_op(op, left, right)?))
        }
        (Value::List(left), Value::List(right)) if left.len() == right.len() => {
            let values = left
                .into_iter()
                .zip(right)
                .map(|(left, right)| elementwise(op, left, right))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(values))
        }
        _ => Ok(Value::Undef),
    }
}

fn dot(left: &[f64], right: &[f64]) -> Option<f64> {
    if left.len() != right.len() {
        return None;
    }
    Some(left.iter().zip(right).map(|(a, b)| a * b).sum())
}

fn multiply(left: Value, right: Value) -> Result<Value, EvalError> {
    if let (Some(left), Some(right)) = (numeric_list(&left), numeric_list(&right)) {
        return Ok(dot(&left, &right)
            .map(Value::Number)
            .unwrap_or(Value::Undef));
    }

    if let (Some(matrix), Some(vector)) = (numeric_matrix(&left), numeric_list(&right)) {
        let mut result = Vec::with_capacity(matrix.len());
        for row in &matrix {
            let Some(value) = dot(row, &vector) else {
                return Ok(Value::Undef);
            };
            result.push(Value::Number(value));
        }
        return Ok(Value::List(result));
    }

    if let (Some(vector), Some(matrix)) = (numeric_list(&left), numeric_matrix(&right)) {
        let Some(width) = matrix.first().map(Vec::len) else {
            return Ok(Value::Undef);
        };
        if matrix.iter().any(|row| row.len() != width) {
            return Ok(Value::Undef);
        }
        let mut result = Vec::with_capacity(width);
        for column in 0..width {
            let mut total = 0.0;
            for (row, value) in matrix.iter().zip(&vector) {
                total += row[column] * value;
            }
            result.push(Value::Number(total));
        }
        return Ok(Value::List(result));
    }

    if let (Some(left), Some(right)) = (numeric_matrix(&left), numeric_matrix(&right)) {
        let Some(width) = right.first().map(Vec::len) else {
            return Ok(Value::Undef);
        };
        if right.iter().any(|row| row.len() != width) {
            return Ok(Value::Undef);
        }
        let mut result = Vec::with_capacity(left.len());
        for row in &left {
            if row.len() != right.len() {
                return Ok(Value::Undef);
            }
            let mut values = vec![0.0; width];
            for (index, value) in row.iter().enumerate() {
                for (total, cell) in values.iter_mut().zip(&right[index]) {
                    *total += value * cell;
                }
            }
            result.push(Value::List(values.into_iter().map(Value::Number).collect()));
        }
        return Ok(Value::List(result));
    }

    Ok(Value::Undef)
}

fn arithmetic(op: SyntaxKind, left: Value, right: Value) -> Result<Value, EvalError> {
    if matches!(left, Value::Undef) || matches!(right, Value::Undef) {
        return Ok(Value::Undef);
    }

    match op {
        SyntaxKind::PLUS | SyntaxKind::MINUS => elementwise(op, left, right),
        SyntaxKind::STAR => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Ok(Value::Number(left * right)),
            (Value::Number(scalar), value) => scale_list(value, scalar, SyntaxKind::STAR),
            (value, Value::Number(scalar)) => scale_list(value, scalar, SyntaxKind::STAR),
            (left @ Value::List(_), right @ Value::List(_)) => multiply(left, right),
            _ => Ok(Value::Undef),
        },
        SyntaxKind::SLASH | SyntaxKind::PERCENT => match (left, right) {
            (Value::Number(left), Value::Number(right)) => {
                Ok(Value::Number(number_op(op, left, right)?))
            }
            (value @ Value::List(_), Value::Number(scalar)) => scale_list(value, scalar, op),
            _ => Ok(Value::Undef),
        },
        SyntaxKind::CARET => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Ok(Value::Number(left.powf(right))),
            _ => Ok(Value::Undef),
        },
        _ => Ok(Value::Undef),
    }
}

fn negate(value: Value) -> Value {
    match value {
        Value::Number(value) => Value::Number(-value),
        Value::List(values) => Value::List(values.into_iter().map(negate).collect()),
        Value::Undef => Value::Undef,
        _ => Value::Undef,
    }
}

pub(crate) fn binary_value(op: SyntaxKind, left: Value, right: Value) -> Result<Value, EvalError> {
    match op {
        SyntaxKind::EQEQ => Ok(Value::Boolean(left == right)),
        SyntaxKind::NE => Ok(Value::Boolean(left != right)),
        SyntaxKind::LT | SyntaxKind::LE | SyntaxKind::GT | SyntaxKind::GE => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Ok(Value::Boolean(match op {
                SyntaxKind::LT => left < right,
                SyntaxKind::LE => left <= right,
                SyntaxKind::GT => left > right,
                _ => left >= right,
            })),
            _ => Ok(Value::Undef),
        },
        SyntaxKind::AND_AND => Ok(Value::Boolean(
            left.as_boolean("&&")? && right.as_boolean("&&")?,
        )),
        SyntaxKind::OR_OR => Ok(Value::Boolean(
            left.as_boolean("||")? || right.as_boolean("||")?,
        )),
        _ => arithmetic(op, left, right),
    }
}

pub(crate) fn unary_value(op: SyntaxKind, value: Value) -> Result<Value, EvalError> {
    match (op, value) {
        (SyntaxKind::PLUS, value) => Ok(value),
        (SyntaxKind::BANG, value) => Ok(Value::Boolean(!value.as_boolean("!")?)),
        (SyntaxKind::MINUS, value) => Ok(negate(value)),
        _ => Err(EvalError::Expression("unsupported operand type".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparisons_return_booleans() {
        assert_eq!(
            binary_value(SyntaxKind::LT, Value::Number(1.0), Value::Number(2.0))
                .expect("comparison"),
            Value::Boolean(true)
        );
        assert_eq!(
            binary_value(
                SyntaxKind::EQEQ,
                Value::List(vec![Value::Number(1.0), Value::String("a".to_owned())]),
                Value::List(vec![Value::Number(1.0), Value::String("a".to_owned())])
            )
            .expect("comparison"),
            Value::Boolean(true)
        );
    }

    #[test]
    fn logical_operators_use_truthiness() {
        assert_eq!(
            binary_value(
                SyntaxKind::AND_AND,
                Value::Boolean(true),
                Value::Number(2.0)
            )
            .expect("and"),
            Value::Boolean(true)
        );
        assert_eq!(
            binary_value(SyntaxKind::OR_OR, Value::Number(0.0), Value::Boolean(true)).expect("or"),
            Value::Boolean(true)
        );
        assert_eq!(
            unary_value(SyntaxKind::BANG, Value::Number(0.0)).expect("not"),
            Value::Boolean(true)
        );
    }

    #[test]
    fn booleans_are_not_numbers() {
        assert!(Value::Boolean(true).as_number("test").is_err());
    }

    #[test]
    fn lists_are_elementwise_but_scalars_do_not_broadcast() {
        assert_eq!(
            binary_value(
                SyntaxKind::PLUS,
                Value::List(vec![Value::Number(1.0), Value::Number(2.0)]),
                Value::List(vec![Value::Number(3.0), Value::Number(4.0)])
            )
            .expect("list arithmetic"),
            Value::List(vec![Value::Number(4.0), Value::Number(6.0)])
        );
        assert_eq!(
            binary_value(
                SyntaxKind::PLUS,
                Value::List(vec![Value::Number(1.0)]),
                Value::Number(2.0)
            )
            .expect("mixed arithmetic"),
            Value::Undef
        );
    }

    #[test]
    fn nested_lists_are_preserved() {
        let value = Value::List(vec![
            Value::List(vec![Value::Number(1.0), Value::String("a".to_owned())]),
            Value::Undef,
        ]);

        assert!(value.as_number("test").is_err());
        assert!(value.as_boolean("test").expect("truthy"));
    }
}
