//! Expression values and arithmetic.

use brepscad_kernel_api::Transform;
use brepscad_syntax::SyntaxKind;

use crate::{EvalError, bad_args};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Number(f64),
    Boolean(bool),
    String(String),
    Vector(Vec<f64>),
    Matrix(Vec<Vec<f64>>),
}

impl Value {
    pub(crate) fn as_number(&self, module: &str) -> Result<f64, EvalError> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Boolean(_) | Self::String(_) | Self::Vector(_) | Self::Matrix(_) => {
                Err(bad_args(module, "expected a number"))
            }
        }
    }

    pub(crate) fn as_boolean(&self, module: &str) -> Result<bool, EvalError> {
        match self {
            Self::Boolean(value) => Ok(*value),
            Self::Number(value) => Ok(*value != 0.0),
            Self::String(_) | Self::Vector(_) | Self::Matrix(_) => {
                Err(bad_args(module, "expected a boolean or number"))
            }
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
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            Self::Boolean(_) | Self::String(_) | Self::Vector(_) | Self::Matrix(_) => {
                Err(bad_args(module, "expected a 3-element vector or a scalar"))
            }
        }
    }

    pub(crate) fn as_size2(&self, module: &str) -> Result<[f64; 2], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 2]),
            Self::Vector(values) if values.len() == 2 => Ok([values[0], values[1]]),
            _ => Err(bad_args(module, "expected a 2-element vector or a scalar")),
        }
    }

    pub(crate) fn as_polygon_points(&self, module: &str) -> Result<Vec<[f64; 2]>, EvalError> {
        let Self::Matrix(rows) = self else {
            return Err(bad_args(module, "expected a list of 2D points"));
        };
        rows.iter()
            .map(|row| match row.as_slice() {
                [x, y] => Ok([*x, *y]),
                _ => Err(bad_args(module, "each point must have two coordinates")),
            })
            .collect()
    }

    pub(crate) fn as_points3(&self, module: &str) -> Result<Vec<[f64; 3]>, EvalError> {
        let Self::Matrix(rows) = self else {
            return Err(bad_args(module, "expected a list of 3D points"));
        };
        rows.iter()
            .map(|row| match row.as_slice() {
                [x, y, z] => Ok([*x, *y, *z]),
                _ => Err(bad_args(module, "each point must have three coordinates")),
            })
            .collect()
    }

    pub(crate) fn as_faces(
        &self,
        module: &str,
        point_count: usize,
    ) -> Result<Vec<Vec<usize>>, EvalError> {
        let Self::Matrix(rows) = self else {
            return Err(bad_args(module, "expected a list of faces"));
        };
        rows.iter()
            .map(|row| {
                if row.len() < 3 {
                    return Err(bad_args(module, "each face needs at least three vertices"));
                }
                row.iter()
                    .map(|value| {
                        if !value.is_finite() || *value < 0.0 || value.fract() != 0.0 {
                            return Err(bad_args(
                                module,
                                "face indices must be non-negative integers",
                            ));
                        }
                        let index = *value as usize;
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
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            _ => Err(bad_args(module, "expected a 3-element vector")),
        }
    }

    pub(crate) fn as_scale(&self, module: &str) -> Result<[f64; 3], EvalError> {
        match self {
            Self::Number(value) => Ok([*value; 3]),
            Self::Vector(values) if values.len() == 3 => Ok([values[0], values[1], values[2]]),
            _ => Err(bad_args(module, "expected a 3-element vector or a scalar")),
        }
    }

    pub(crate) fn as_matrix(&self, module: &str) -> Result<Transform, EvalError> {
        let Self::Matrix(rows) = self else {
            return Err(bad_args(module, "expected a 4x3 or 4x4 matrix"));
        };
        if !(rows.len() == 3 || rows.len() == 4) || rows.iter().any(|row| row.len() != 4) {
            return Err(bad_args(module, "expected a 4x3 or 4x4 matrix"));
        }

        let mut matrix = [[0.0; 4]; 4];
        for (row, values) in rows.iter().take(3).enumerate() {
            matrix[row].copy_from_slice(values);
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
        SyntaxKind::SLASH => {
            if right == 0.0 {
                Err(EvalError::Expression("division by zero".to_owned()))
            } else {
                Ok(left / right)
            }
        }
        SyntaxKind::PERCENT => {
            if right == 0.0 {
                Err(EvalError::Expression("division by zero".to_owned()))
            } else {
                Ok(left % right)
            }
        }
        _ => Err(EvalError::Expression(format!(
            "unsupported operator {op:?}"
        ))),
    }
}

pub(crate) fn binary_value(op: SyntaxKind, left: Value, right: Value) -> Result<Value, EvalError> {
    match op {
        SyntaxKind::EQEQ => Ok(Value::Boolean(left == right)),
        SyntaxKind::NE => Ok(Value::Boolean(left != right)),
        SyntaxKind::LT | SyntaxKind::LE | SyntaxKind::GT | SyntaxKind::GE => {
            let (Value::Number(left), Value::Number(right)) = (left, right) else {
                return Err(EvalError::Expression(
                    "comparison requires numbers".to_owned(),
                ));
            };
            Ok(Value::Boolean(match op {
                SyntaxKind::LT => left < right,
                SyntaxKind::LE => left <= right,
                SyntaxKind::GT => left > right,
                _ => left >= right,
            }))
        }
        SyntaxKind::AND_AND => Ok(Value::Boolean(
            left.as_boolean("&&")? && right.as_boolean("&&")?,
        )),
        SyntaxKind::OR_OR => Ok(Value::Boolean(
            left.as_boolean("||")? || right.as_boolean("||")?,
        )),
        _ => match (left, right) {
            (Value::Number(left), Value::Number(right)) => {
                Ok(Value::Number(number_op(op, left, right)?))
            }
            (Value::Vector(left), Value::Vector(right)) if left.len() == right.len() => {
                let values = left
                    .iter()
                    .zip(&right)
                    .map(|(left, right)| number_op(op, *left, *right))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Vector(values))
            }
            (Value::Vector(values), Value::Number(scalar)) => Ok(Value::Vector(
                values
                    .iter()
                    .map(|value| number_op(op, *value, scalar))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            (Value::Number(scalar), Value::Vector(values)) => Ok(Value::Vector(
                values
                    .iter()
                    .map(|value| number_op(op, scalar, *value))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => Err(EvalError::Expression(
                "unsupported operand types".to_owned(),
            )),
        },
    }
}

pub(crate) fn unary_value(op: SyntaxKind, value: Value) -> Result<Value, EvalError> {
    match (op, value) {
        (SyntaxKind::PLUS, value) => Ok(value),
        (SyntaxKind::BANG, value) => Ok(Value::Boolean(!value.as_boolean("!")?)),
        (SyntaxKind::MINUS, Value::Number(value)) => Ok(Value::Number(-value)),
        (SyntaxKind::MINUS, Value::Vector(values)) => Ok(Value::Vector(
            values.into_iter().map(|value| -value).collect(),
        )),
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
            binary_value(SyntaxKind::GE, Value::Number(1.0), Value::Number(2.0))
                .expect("comparison"),
            Value::Boolean(false)
        );
        assert_eq!(
            binary_value(
                SyntaxKind::EQEQ,
                Value::Vector(vec![1.0, 2.0]),
                Value::Vector(vec![1.0, 2.0])
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
}
