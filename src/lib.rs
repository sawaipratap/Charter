//! Charter: a programming language where function signatures declare effects.
//!
//! This library provides the core parsing and evaluation machinery.
//! The CLI binary lives in `src/main.rs`.

mod eval;
mod lexer;
mod parse;

pub use eval::eval;
pub use lexer::lex;
pub use parse::parse;

/// A value in Charter's Stage 0: either an integer or an exact rational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Integer(i64),
    Rational(i64, i64), // numerator, denominator (denominator always > 0)
}

impl Value {
    /// Evaluate a binary operation on two values.
    pub fn apply_op(self, op: char, rhs: Value) -> Result<Value, String> {
        match op {
            '+' => self.add(rhs),
            '-' => self.sub(rhs),
            '*' => self.mul(rhs),
            '/' => self.div(rhs),
            _ => Err(format!("unknown operator '{}'", op)),
        }
    }

    fn add(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational();
        let b = rhs.to_rational();
        let num = a.0.checked_mul(a.1)
            .and_then(|a_num| b.0.checked_mul(b.1).and_then(|b_num| a_num.checked_add(b_num)))
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    fn sub(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational();
        let b = rhs.to_rational();
        let num = a.0.checked_mul(a.1)
            .and_then(|a_num| b.0.checked_mul(b.1).and_then(|b_num| a_num.checked_sub(b_num)))
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    fn mul(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational();
        let b = rhs.to_rational();
        let num = a.0.checked_mul(b.0)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        let den = a.1.checked_mul(b.1)
            .ok_or_else(|| "runtime error: integer overflow".to_string())?;
        Ok(Value::reduce(num, den))
    }

    fn div(self, rhs: Value) -> Result<Value, String> {
        let a = self.to_rational();
        let b = rhs.to_rational();
        if b.0 == 0 {
            return Err("runtime error: division by zero".to_string());
        }
        Ok(Value::reduce(a.0 * b.1, a.1 * b.0))
    }

    fn to_rational(&self) -> (i64, i64) {
        match self {
            Value::Integer(n) => (*n, 1),
            Value::Rational(n, d) => (*n, *d),
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
            Value::Integer(n) => Value::Integer(-n),
            Value::Rational(n, d) => Value::Rational(-n, d),
        }
    }
}
