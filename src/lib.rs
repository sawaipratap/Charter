//! Charter: a programming language where function signatures declare effects.

use std::collections::HashMap;

mod eval;
mod lexer;
mod parse;

pub use eval::{eval_program, eval_string};
pub use lexer::lex;
pub use parse::{parse, parse_program, Program, Expr, BinOp, Function, Effect, Operation};
pub use lexer::Token;

/// A continuation for effect handlers (Stage 3).
/// Captures the rest of the computation after a perform.
/// When k(value) is called, the continuation is resumed with value.
#[derive(Clone, PartialEq, Eq)]
pub struct Continuation {
    /// The pending expression to evaluate after receiving the value
    pub pending: Expr,
    /// The environment at the point of perform
    pub env: HashMap<String, Value>,
    /// The functions registry for the continuation
    pub functions: HashMap<String, Function>,
}



impl std::fmt::Debug for Continuation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Continuation")
            .field("pending", &"<expr>")
            .field("env", &"<env>")
            .finish()
    }
}

/// A value in Charter: an integer, exact rational, boolean, or continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Integer(i64),
    Rational(i64, i64), // numerator, denominator (denominator always > 0)
    Bool(bool),
    Continuation(Box<Continuation>),
}

impl Value {
    pub fn add(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational()?;
        let b = rhs.to_rational()?;
        let num = a.0.checked_mul(a.1)
            .and_then(|a_num| b.0.checked_mul(b.1).and_then(|b_num| a_num.checked_add(b_num)))
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    pub fn sub(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational()?;
        let b = rhs.to_rational()?;
        let num = a.0.checked_mul(a.1)
            .and_then(|a_num| b.0.checked_mul(b.1).and_then(|b_num| a_num.checked_sub(b_num)))
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    pub fn mul(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational()?;
        let b = rhs.to_rational()?;
        let num = a.0.checked_mul(b.0)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    pub fn div(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational()?;
        let b = rhs.to_rational()?;
        if b.0 == 0 {
            return Err("runtime error: division by zero".to_string());
        }
        Ok(Value::reduce(a.0 * b.1, a.1 * b.0))
    }

    fn to_rational(&self) -> Result<(i64, i64), String> {
        match self {
            Value::Integer(n) => Ok((*n, 1)),
            Value::Rational(n, d) => Ok((*n, *d)),
            Value::Bool(_) => Err("runtime error: arithmetic on booleans".to_string()),
            Value::Continuation(_) => Err("runtime error: arithmetic on continuations".to_string()),
        }
    }

    fn reduce(num: i64, den: i64) -> Value {
        if den == 0 {
            panic!("zero denominator");
        }
        let (num, den) = if den < 0 {
            (-num, -den)
        } else {
            (num, den)
        };
        let g = gcd(num.abs(), den);
        let num = num / g;
        let den = den / g;
        if den == 1 {
            Value::Integer(num)
        } else {
            Value::Rational(num, den)
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Integer(n) => n.to_string(),
            Value::Rational(n, d) => format!("{}/{}", n, d),
            Value::Bool(b) => b.to_string(),
            Value::Continuation(_) => "<continuation>".to_string(),
        }
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

impl std::ops::Neg for Value {
    type Output = Value;
    fn neg(self) -> Value {
        match self {
            Value::Integer(n) => Value::Integer(n.checked_neg().unwrap_or(n)),
            Value::Rational(n, d) => Value::Rational(-n, d),
            Value::Bool(_) => panic!("cannot negate boolean"),
            Value::Continuation(_) => panic!("cannot negate continuation"),
        }
    }
}
