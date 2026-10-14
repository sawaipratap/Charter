//! Evaluator: run an expression and produce a value.
//!
//! At Stage 0, there's no environment (no variables), so evaluation is straightforward:
//! walk the tree, apply operations, and return the result.

use crate::parse::Expr;
use crate::Value;

/// Evaluate an expression and return its value.
/// Division produces exact rationals (e.g., 7/2 stays as 7/2 until simplified).
pub fn eval(expr: Expr) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Integer(n)),

        Expr::UnaryMinus(inner) => {
            let val = eval(*inner)?;
            Ok(-val)
        }

        Expr::Binary(left, op, right) => {
            let lval = eval(*left)?;
            let rval = eval(*right)?;
            lval.apply_op(op, rval)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Token;
    use crate::parse;

    #[test]
    fn test_literal() {
        let ast = parse(vec![Token::Number(42)]).unwrap();
        assert_eq!(eval(ast).unwrap().display(), "42");
    }

    #[test]
    fn test_add() {
        let tokens = vec![Token::Number(1), Token::Plus, Token::Number(2)];
        let ast = parse(tokens).unwrap();
        assert_eq!(eval(ast).unwrap().display(), "3");
    }

    #[test]
    fn test_precedence() {
        // 2 + 3 * 4 should be 14, not 20
        let tokens = vec![Token::Number(2), Token::Plus, Token::Number(3), Token::Star, Token::Number(4)];
        let ast = parse(tokens).unwrap();
        assert_eq!(eval(ast).unwrap().display(), "14");
    }

    #[test]
    fn test_exact_division() {
        // 7 / 2 should stay as 7/2
        let tokens = vec![Token::Number(7), Token::Slash, Token::Number(2)];
        let ast = parse(tokens).unwrap();
        assert_eq!(eval(ast).unwrap().display(), "7/2");
    }

    #[test]
    fn test_division_that_reduces() {
        // 6 / 4 should simplify to 3/2
        let tokens = vec![Token::Number(6), Token::Slash, Token::Number(4)];
        let ast = parse(tokens).unwrap();
        assert_eq!(eval(ast).unwrap().display(), "3/2");
    }

    #[test]
    fn test_division_by_zero() {
        let tokens = vec![Token::Number(1), Token::Slash, Token::Number(0)];
        let ast = parse(tokens).unwrap();
        assert!(eval(ast).is_err());
    }
}
