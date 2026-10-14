//! Lexer: turn raw text into a stream of tokens.
//!
//! A token is the smallest meaningful unit — a number, an operator, a parenthesis.

use std::iter::Peekable;
use std::str::Chars;

/// All the different kinds of tokens Charter understands at Stage 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Number(i64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

/// Turn a string into a flat list of tokens.
/// Returns an error if the input contains any character that isn't a valid token.
pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            '0'..='9' => {
                let n = read_number(&mut chars);
                tokens.push(Token::Number(n));
            }
            '+' => { tokens.push(Token::Plus); chars.next(); }
            '-' => { tokens.push(Token::Minus); chars.next(); }
            '*' => { tokens.push(Token::Star); chars.next(); }
            '/' => { tokens.push(Token::Slash); chars.next(); }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            c if c.is_whitespace() => { chars.next(); }
            c => return Err(format!("lex error: unexpected character '{}'", c)),
        }
    }

    Ok(tokens)
}

/// Read one multi-digit number from the character iterator.
fn read_number(chars: &mut Peekable<Chars>) -> i64 {
    let mut n = 0i64;
    while let Some(&c) = chars.peek() {
        if let Some(d) = c.to_digit(10) {
            n = n * 10 + d as i64;
            chars.next();
        } else {
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        assert_eq!(lex("42"), Ok(vec![Token::Number(42)]));
    }

    #[test]
    fn test_ops() {
        assert_eq!(
            lex("1 + 2"),
            Ok(vec![Token::Number(1), Token::Plus, Token::Number(2)])
        );
    }

    #[test]
    fn test_parens() {
        assert_eq!(
            lex("(1)"),
            Ok(vec![Token::LParen, Token::Number(1), Token::RParen])
        );
    }
}
