//! Parser: turn a token stream into an abstract syntax tree (AST).
//!
//! The tree captures the correct precedence: * and / bind tighter than + and -.
//! We use a two-level recursive descent, which is the standard way to handle
//! operator precedence without a complex parser generator.

use crate::lexer::Token;

/// An expression in Charter's Stage 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal number (always non-negative at the lexing stage; unary minus handles negation).
    Number(i64),
    /// A binary operation: left op right.
    Binary(Box<Expr>, char, Box<Expr>),
    /// A unary negation: -expr
    UnaryMinus(Box<Expr>),
}

/// Turn a token list into an expression tree.
/// Fails if the tokens don't form a valid expression.
pub fn parse(tokens: Vec<Token>) -> Result<Expr, String> {
    if tokens.is_empty() {
        return Err("parse error: empty input".to_string());
    }
    let mut t = tokens.into_iter().peekable();
    let expr = parse_expr(&mut t)?;
    if t.next().is_some() {
        return Err("parse error: unexpected tokens after complete expression".to_string());
    }
    Ok(expr)
}

/// Parse a full expression (lowest precedence: + and -).
/// Keeps going until it hits something of lower precedence or end.
fn parse_expr(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_term(tokens)?;

    loop {
        match tokens.peek() {
            Some(Token::Plus) | Some(Token::Minus) => {
                let op = match tokens.next() {
                    Some(Token::Plus) => '+',
                    Some(Token::Minus) => '-',
                    _ => unreachable!(),
                };
                let right = parse_term(tokens)?;
                left = Expr::Binary(Box::new(left), op, Box::new(right));
            }
            _ => break,
        }
    }

    Ok(left)
}

/// Parse a term (higher precedence: * and /).
fn parse_term(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_factor(tokens)?;

    loop {
        match tokens.peek() {
            Some(Token::Star) | Some(Token::Slash) => {
                let op = match tokens.next() {
                    Some(Token::Star) => '*',
                    Some(Token::Slash) => '/',
                    _ => unreachable!(),
                };
                let right = parse_factor(tokens)?;
                left = Expr::Binary(Box::new(left), op, Box::new(right));
            }
            _ => break,
        }
    }

    Ok(left)
}

/// Parse a factor: a number, a parenthesized expression, or a unary minus.
/// Unary minus can stack: --5 is valid.
fn parse_factor(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    match tokens.next() {
        Some(Token::Number(n)) => Ok(Expr::Number(n)),
        Some(Token::LParen) => {
            let inner = parse_expr(tokens)?;
            match tokens.next() {
                Some(Token::RParen) => Ok(inner),
                Some(t) => Err(format!("parse error: unexpected token after expression: {:?}", t)),
                None => Err("parse error: unclosed parenthesis".to_string()),
            }
        }
        Some(Token::Minus) => {
            let inner = parse_factor(tokens)?;
            Ok(Expr::UnaryMinus(Box::new(inner)))
        }
        Some(t) => Err(format!("parse error: unexpected token: {:?}", t)),
        None => Err("parse error: unexpected end of input".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number() {
        assert_eq!(parse(vec![crate::lexer::Token::Number(42)]), Ok(Expr::Number(42)));
    }

    #[test]
    fn test_add() {
        let tokens = vec![Token::Number(1), Token::Plus, Token::Number(2)];
        let ast = parse(tokens).unwrap();
        assert_eq!(ast, Expr::Binary(Box::new(Expr::Number(1)), '+', Box::new(Expr::Number(2))));
    }

    #[test]
    fn test_precedence() {
        let tokens = vec![Token::Number(1), Token::Plus, Token::Number(2), Token::Star, Token::Number(3)];
        let ast = parse(tokens).unwrap();
        match ast {
            Expr::Binary(left, '+', right) => {
                assert!(matches!(*left, Expr::Number(1)));
                assert!(matches!(*right, Expr::Binary(_, '*', _)));
            }
            _ => panic!("expected binary +"),
        }
    }
}
