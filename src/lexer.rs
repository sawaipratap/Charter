//! Lexer: turn raw text into a stream of tokens.

use std::iter::Peekable;
use std::str::Chars;

/// All the different kinds of tokens Charter understands at Stage 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // Literals
    Number(i64),
    Boolean(bool),
    Ident(String),
    
    // Keywords
    Fn,
    Let,
    If,
    Else,
    True,
    False,
    Effect,
    Uses,
    Perform,
    Handle,
    With,
    
    // Operators
    Arrow,  // =>
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,  // && 
    Or,   // ||
    Bang, // !
    
    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Assign,  // =
    Colon,   // :
    
    // Special
    Eof,
}

/// Turn a string into a flat list of tokens.
pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            '0'..='9' => {
                let n = read_number(&mut chars)?;
                tokens.push(Token::Number(n));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let ident = read_ident(&mut chars);
                let token = match ident.as_str() {
                    "fn" => Token::Fn,
                    "let" => Token::Let,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "true" => Token::True,
                    "false" => Token::False,
                    "effect" => Token::Effect,
                    "uses" => Token::Uses,
                    "perform" => Token::Perform,
                    "handle" => Token::Handle,
                    "with" => Token::With,
                    _ => Token::Ident(ident),
                };
                tokens.push(token);
            }
            '+' => { tokens.push(Token::Plus); chars.next(); }
            '-' => { tokens.push(Token::Minus); chars.next(); }
            '*' => { tokens.push(Token::Star); chars.next(); }
            '/' => {
                chars.next();
                // Check for comment
                if chars.peek() == Some(&'/') {
                    // Comment: skip until end of line
                    while chars.peek() != Some(&'\n') && chars.peek().is_some() {
                        chars.next();
                    }
                } else {
                    tokens.push(Token::Slash);
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::LessEqual);
                } else {
                    tokens.push(Token::Less);
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::Greater);
                }
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Equal);
                } else if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(Token::Arrow);
                } else {
                    tokens.push(Token::Assign);
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::NotEqual);
                } else {
                    tokens.push(Token::Bang);
                }
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token::And);
                } else {
                    return Err("lex error: unexpected character '&' (did you mean '&&'?)".to_string());
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::Or);
                } else {
                    return Err("lex error: unexpected character '|' (did you mean '||'?)".to_string());
                }
            }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            '{' => { tokens.push(Token::LBrace); chars.next(); }
            '}' => { tokens.push(Token::RBrace); chars.next(); }
            '[' => { tokens.push(Token::LBracket); chars.next(); }
            ']' => { tokens.push(Token::RBracket); chars.next(); }
            ';' => { tokens.push(Token::Semicolon); chars.next(); }
            ',' => { tokens.push(Token::Comma); chars.next(); }
            ':' => { tokens.push(Token::Colon); chars.next(); }
            c if c.is_whitespace() => { chars.next(); }
            c => return Err(format!("lex error: unexpected character '{}'", c)),
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

/// Read one multi-digit number from the character iterator.
fn read_number(chars: &mut Peekable<Chars>) -> Result<i64, String> {
    let mut n = 0i64;
    while let Some(&c) = chars.peek() {
        if let Some(d) = c.to_digit(10) {
            n = n.checked_mul(10).and_then(|n| n.checked_add(d as i64))
                .ok_or_else(|| "lex error: integer literal too big".to_string())?;
            chars.next();
        } else {
            break;
        }
    }
    Ok(n)
}

/// Read an identifier.
fn read_ident(chars: &mut Peekable<Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        assert_eq!(lex("42"), Ok(vec![Token::Number(42), Token::Eof]));
    }

    #[test]
    fn test_keywords() {
        assert_eq!(lex("fn let if else true false"), 
            Ok(vec![Token::Fn, Token::Let, Token::If, Token::Else, Token::True, Token::False, Token::Eof]));
    }
}
