//! Parser: turn a token stream into an abstract syntax tree (AST).

use crate::lexer::Token;
use std::collections::{HashMap, HashSet};

/// A function definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

/// A complete program: function definitions followed by a main expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub functions: Vec<Function>,
    pub main: Expr,
}

/// An expression in Charter's Stage 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    // Literals
    Number(i64),
    Boolean(bool),
    Ident(String),
    
    // Block: let bindings followed by final expression
    Block(Vec<(String, Expr)>, Box<Expr>),
    
    // Function call
    Call(String, Vec<Expr>),
    
    // Binary operations
    Binary(Box<Expr>, BinOp, Box<Expr>),
    
    // Unary operations
    UnaryMinus(Box<Expr>),
    Not(Box<Expr>),
    
    // If expression
    If(Box<Expr>, Box<Expr>, Box<Expr>), // condition, then, else
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add, Sub, Mul, Div,
    // Comparison
    Less, LessEqual, Greater, GreaterEqual, Equal, NotEqual,
    // Logical
    And, Or,
}

impl BinOp {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(BinOp::Add),
            '-' => Some(BinOp::Sub),
            '*' => Some(BinOp::Mul),
            '/' => Some(BinOp::Div),
            _ => None,
        }
    }
    
    pub fn from_token(token: &Token) -> Option<Self> {
        match token {
            Token::Plus => Some(BinOp::Add),
            Token::Minus => Some(BinOp::Sub),
            Token::Star => Some(BinOp::Mul),
            Token::Slash => Some(BinOp::Div),
            Token::Less => Some(BinOp::Less),
            Token::LessEqual => Some(BinOp::LessEqual),
            Token::Greater => Some(BinOp::Greater),
            Token::GreaterEqual => Some(BinOp::GreaterEqual),
            Token::Equal => Some(BinOp::Equal),
            Token::NotEqual => Some(BinOp::NotEqual),
            Token::And => Some(BinOp::And),
            Token::Or => Some(BinOp::Or),
            _ => None,
        }
    }
    
    pub fn is_comparison(&self) -> bool {
        matches!(self, BinOp::Less | BinOp::LessEqual | BinOp::Greater | 
                      BinOp::GreaterEqual | BinOp::Equal | BinOp::NotEqual)
    }
}

/// Parse a token list as a complete program.
pub fn parse(tokens: Vec<Token>) -> Result<Program, String> {
    parse_program(tokens)
}

pub fn parse_program(tokens: Vec<Token>) -> Result<Program, String> {
    let mut tokens = tokens.into_iter().peekable();
    
    // Parse functions and main expression in any order
    let mut functions = Vec::new();
    let mut seen_fn_names: HashMap<String, ()> = HashMap::new();
    let mut main_expr = None;
    
    loop {
        match tokens.peek() {
            Some(Token::Fn) => {
                let func = parse_fn(&mut tokens, &mut seen_fn_names)?;
                functions.push(func);
            }
            Some(Token::Eof) => {
                tokens.next();
                break;
            }
            Some(_) => {
                // Main expression
                if main_expr.is_some() {
                    return Err("parse error: multiple expressions at top level".to_string());
                }
                main_expr = Some(parse_expr(&mut tokens)?);
                if tokens.peek() != Some(&Token::Eof) {
                    // Could be a function after the expression
                    if tokens.peek() == Some(&Token::Fn) {
                        continue;
                    }
                    return Err("parse error: unexpected tokens after expression".to_string());
                }
            }
            None => break,
        }
    }
    
    let main = main_expr.ok_or_else(|| "parse error: no expression found".to_string())?;
    
    // Static checks: build function map and check references
    let fn_map: HashMap<String, usize> = functions.iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    
    check_static(&main, &fn_map)?;
    for func in &functions {
        // Function parameters are bound in the function body
        let mut param_set: HashSet<String> = HashSet::new();
        for param in &func.params {
            param_set.insert(param.clone());
        }
        check_static_with_scope(&func.body, &fn_map, &param_set)?;
    }
    
    Ok(Program { functions, main })
}

/// Static checks: function references and variable scope
fn check_static(expr: &Expr, fn_map: &HashMap<String, usize>) -> Result<HashSet<String>, String> {
    check_static_with_scope(expr, fn_map, &HashSet::new())
}

fn check_static_with_scope(
    expr: &Expr, 
    fn_map: &HashMap<String, usize>,
    bound_vars: &HashSet<String>
) -> Result<HashSet<String>, String> {
    match expr {
        Expr::Number(_) | Expr::Boolean(_) => Ok(bound_vars.clone()),
        Expr::Ident(name) => {
            // Check if variable is bound
            if !bound_vars.contains(name) {
                return Err(format!("check error: unbound variable '{}'", name));
            }
            Ok(bound_vars.clone())
        }
        Expr::Block(bindings, body) => {
            let mut new_bound = bound_vars.clone();
            for (name, e) in bindings {
                // The right side of let binding can see current bindings
                check_static_with_scope(e, fn_map, &new_bound)?;
                // Shadowing is allowed - just add the new binding
                new_bound.insert(name.clone());
            }
            // Body can see all bindings
            check_static_with_scope(body, fn_map, &new_bound)
        }
        Expr::Call(name, args) => {
            // Check function exists
            if !fn_map.contains_key(name) {
                return Err(format!("check error: undefined function '{}'", name));
            }
            // Check arguments (arity checked at runtime with function def)
            let mut new_bound = bound_vars.clone();
            for arg in args {
                new_bound = check_static_with_scope(arg, fn_map, &new_bound)?;
            }
            Ok(new_bound)
        }
        Expr::Binary(left, _, right) => {
            let new_bound = check_static_with_scope(left, fn_map, bound_vars)?;
            check_static_with_scope(right, fn_map, &new_bound)
        }
        Expr::UnaryMinus(inner) | Expr::Not(inner) => {
            check_static_with_scope(inner, fn_map, bound_vars)
        }
        Expr::If(cond, then, else_) => {
            let new_bound = check_static_with_scope(cond, fn_map, bound_vars)?;
            let new_bound = check_static_with_scope(then, fn_map, &new_bound)?;
            check_static_with_scope(else_, fn_map, &new_bound)
        }
    }
}

/// Parse a function definition: fn name(params) { body }
fn parse_fn(
    tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>, 
    seen_names: &mut HashMap<String, ()>
) -> Result<Function, String> {
    // Consume 'fn'
    match tokens.next() {
        Some(Token::Fn) => {}
        Some(t) => return Err(format!("parse error: expected 'fn', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Function name
    let name = match tokens.next() {
        Some(Token::Ident(s)) => s,
        Some(t) => return Err(format!("parse error: expected function name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    };
    
    // Check for duplicate function names
    if seen_names.contains_key(&name) {
        return Err(format!("check error: duplicate function name '{}'", name));
    }
    seen_names.insert(name.clone(), ());
    
    // Parameters: (param1, param2, ...)
    match tokens.next() {
        Some(Token::LParen) => {}
        Some(t) => return Err(format!("parse error: expected '(' after function name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    let mut params = Vec::new();
    let mut seen_params: HashMap<String, ()> = HashMap::new();
    
    // Parse parameter list (may be empty)
    // No trailing comma allowed
    let mut first_param = true;
    while tokens.peek() != Some(&Token::RParen) {
        if !first_param {
            // Expect comma before next parameter
            match tokens.next() {
                Some(Token::Comma) => {}
                Some(t) => return Err(format!("parse error: expected ',' between parameters, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
        }
        first_param = false;
        
        // Now expect a parameter name
        match tokens.next() {
            Some(Token::Ident(s)) => {
                if seen_params.contains_key(&s) {
                    return Err(format!("check error: duplicate parameter name '{}'", s));
                }
                params.push(s.clone());
                seen_params.insert(s, ());
            }
            Some(Token::RParen) => {
                // Empty parameter or trailing comma
                if params.is_empty() {
                    break;
                } else {
                    return Err("parse error: trailing comma in parameter list".to_string());
                }
            }
            Some(Token::Comma) => {
                return Err("parse error: trailing comma in parameter list".to_string());
            }
            Some(t) => return Err(format!("parse error: expected parameter name, got {:?}", t)),
            None => return Err("parse error: unexpected end of input".to_string()),
        }
    }
    
    // Consume ')'
    match tokens.next() {
        Some(Token::RParen) => {}
        Some(t) => return Err(format!("parse error: expected ')' after parameters, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Function body (a block)
    let body = parse_block(tokens)?;
    
    Ok(Function { name, params, body })
}

/// Parse a block: { let x = expr; let y = expr; final_expr }
fn parse_block(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    match tokens.next() {
        Some(Token::LBrace) => {}
        Some(t) => return Err(format!("parse error: expected '{{', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    let mut lets = Vec::new();
    
    // Parse let bindings
    while tokens.peek() == Some(&Token::Let) {
        let binding = parse_let(tokens)?;
        lets.push(binding);
    }
    
    // Parse final expression
    let final_expr = parse_expr(tokens)?;
    
    match tokens.next() {
        Some(Token::RBrace) => {}
        Some(t) => return Err(format!("parse error: expected '}}' at end of block, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    Ok(Expr::Block(lets, Box::new(final_expr)))
}

/// Parse a let binding: let name = expr;
fn parse_let(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<(String, Expr), String> {
    match tokens.next() {
        Some(Token::Let) => {}
        Some(t) => return Err(format!("parse error: expected 'let', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Variable name
    let name = match tokens.next() {
        Some(Token::Ident(s)) => s,
        Some(t) => return Err(format!("parse error: expected variable name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    };
    
    // '='
    match tokens.next() {
        Some(Token::Assign) => {}
        Some(t) => return Err(format!("parse error: expected '=', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Expression
    let value = parse_expr(tokens)?;
    
    // ';'
    match tokens.next() {
        Some(Token::Semicolon) => {}
        Some(t) => return Err(format!("parse error: expected ';' after let binding, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    Ok((name, value))
}

/// Parse an expression with precedence climbing.
/// Precedence (lowest to highest):
///   or
///   and
///   equality (==, !=)
///   comparison (<, <=, >, >=)
///   addition, subtraction
///   multiplication, division
///   unary (-, !)
///   primary (number, identifier, call, block, if)
fn parse_expr(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    parse_or(tokens)
}

fn parse_or(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_and(tokens)?;
    
    while tokens.peek() == Some(&Token::Or) {
        tokens.next();
        let right = parse_and(tokens)?;
        left = Expr::Binary(Box::new(left), BinOp::Or, Box::new(right));
    }
    
    Ok(left)
}

fn parse_and(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_equality(tokens)?;
    
    while tokens.peek() == Some(&Token::And) {
        tokens.next();
        let right = parse_equality(tokens)?;
        left = Expr::Binary(Box::new(left), BinOp::And, Box::new(right));
    }
    
    Ok(left)
}

fn parse_equality(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_comparison(tokens)?;
    
    loop {
        match tokens.peek() {
            Some(Token::Equal) => {
                tokens.next();
                let right = parse_comparison(tokens)?;
                // Check for chained comparison: left can't be a comparison
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::Equal, Box::new(right));
            }
            Some(Token::NotEqual) => {
                tokens.next();
                let right = parse_comparison(tokens)?;
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::NotEqual, Box::new(right));
            }
            _ => break,
        }
    }
    
    Ok(left)
}

fn parse_comparison(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_addsub(tokens)?;
    
    loop {
        match tokens.peek() {
            Some(Token::Less) => {
                tokens.next();
                let right = parse_addsub(tokens)?;
                // Check for chained comparison: left can't be a comparison
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::Less, Box::new(right));
            }
            Some(Token::LessEqual) => {
                tokens.next();
                let right = parse_addsub(tokens)?;
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::LessEqual, Box::new(right));
            }
            Some(Token::Greater) => {
                tokens.next();
                let right = parse_addsub(tokens)?;
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::Greater, Box::new(right));
            }
            Some(Token::GreaterEqual) => {
                tokens.next();
                let right = parse_addsub(tokens)?;
                if let Expr::Binary(_, op, _) = &left {
                    if op.is_comparison() {
                        return Err("parse error: chained comparison".to_string());
                    }
                }
                left = Expr::Binary(Box::new(left), BinOp::GreaterEqual, Box::new(right));
            }
            _ => break,
        }
    }
    
    Ok(left)
}

fn parse_addsub(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_muldiv(tokens)?;
    
    loop {
        match tokens.peek() {
            Some(Token::Plus) => {
                tokens.next();
                let right = parse_muldiv(tokens)?;
                left = Expr::Binary(Box::new(left), BinOp::Add, Box::new(right));
            }
            Some(Token::Minus) => {
                tokens.next();
                let right = parse_muldiv(tokens)?;
                left = Expr::Binary(Box::new(left), BinOp::Sub, Box::new(right));
            }
            _ => break,
        }
    }
    
    Ok(left)
}

fn parse_muldiv(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let mut left = parse_unary(tokens)?;
    
    loop {
        match tokens.peek() {
            Some(Token::Star) => {
                tokens.next();
                let right = parse_unary(tokens)?;
                left = Expr::Binary(Box::new(left), BinOp::Mul, Box::new(right));
            }
            Some(Token::Slash) => {
                tokens.next();
                let right = parse_unary(tokens)?;
                left = Expr::Binary(Box::new(left), BinOp::Div, Box::new(right));
            }
            _ => break,
        }
    }
    
    Ok(left)
}

fn parse_unary(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    match tokens.peek() {
        Some(Token::Minus) => {
            tokens.next();
            let inner = parse_unary(tokens)?;
            Ok(Expr::UnaryMinus(Box::new(inner)))
        }
        Some(Token::Bang) => {
            tokens.next();
            let inner = parse_unary(tokens)?;
            Ok(Expr::Not(Box::new(inner)))
        }
        _ => parse_primary(tokens),
    }
}

/// Parse an if expression without the leading 'if' token.
/// Used for parsing 'else if' branches.
fn parse_if_expr(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    // This is called after we've consumed the 'if' token
    let cond = parse_expr(tokens)?;
    
    // Then branch (block)
    let then_branch = parse_block(tokens)?;
    
    // else
    match tokens.next() {
        Some(Token::Else) => {}
        Some(t) => return Err(format!("parse error: expected 'else' after if branch, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // else branch: could be a block OR another if expression
    let else_branch = if tokens.peek() == Some(&Token::If) {
        tokens.next();
        parse_if_expr(tokens)?
    } else {
        parse_block(tokens)?
    };
    
    Ok(Expr::If(Box::new(cond), Box::new(then_branch), Box::new(else_branch)))
}

fn parse_primary(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    match tokens.next() {
        Some(Token::Number(n)) => Ok(Expr::Number(n)),
        Some(Token::True) => Ok(Expr::Boolean(true)),
        Some(Token::False) => Ok(Expr::Boolean(false)),
        Some(Token::Ident(s)) => {
            // Check if it's a function call
            if tokens.peek() == Some(&Token::LParen) {
                tokens.next(); // consume '('
                let mut args = Vec::new();
                while tokens.peek() != Some(&Token::RParen) {
                    if args.is_empty() {
                        args.push(parse_expr(tokens)?);
                    } else {
                        match tokens.next() {
                            Some(Token::Comma) => {
                                args.push(parse_expr(tokens)?);
                            }
                            Some(Token::RParen) => break,
                            Some(t) => return Err(format!("parse error: expected ',' or ')' in call, got {:?}", t)),
                            None => return Err("parse error: unexpected end of input".to_string()),
                        }
                    }
                }
                match tokens.next() {
                    Some(Token::RParen) => Ok(Expr::Call(s, args)),
                    Some(t) => Err(format!("parse error: expected ')' after call arguments, got {:?}", t)),
                    None => Err("parse error: unexpected end of input".to_string()),
                }
            } else {
                Ok(Expr::Ident(s))
            }
        }
        Some(Token::LParen) => {
            let inner = parse_expr(tokens)?;
            match tokens.next() {
                Some(Token::RParen) => Ok(inner),
                Some(t) => Err(format!("parse error: expected ')' after expression, got {:?}", t)),
                None => Err("parse error: unexpected end of input".to_string()),
            }
        }
        Some(Token::LBrace) => {
            let mut lets = Vec::new();
            
            // Parse let bindings
            while tokens.peek() == Some(&Token::Let) {
                let binding = parse_let(tokens)?;
                lets.push(binding);
            }
            
            // Parse final expression
            let final_expr = parse_expr(tokens)?;
            
            match tokens.next() {
                Some(Token::RBrace) => {
                    if lets.is_empty() {
                        Ok(final_expr)
                    } else {
                        Ok(Expr::Block(lets, Box::new(final_expr)))
                    }
                }
                Some(t) => Err(format!("parse error: expected '}}' at end of block, got {:?}", t)),
                None => Err("parse error: unexpected end of input".to_string()),
            }
        }
        Some(Token::If) => {
            // if expr { block } else { block } OR if expr { block } else if ...
            let cond = parse_expr(tokens)?;
            
            // Then branch (block)
            let then_branch = parse_block(tokens)?;
            
            // else
            match tokens.next() {
                Some(Token::Else) => {}
                Some(t) => return Err(format!("parse error: expected 'else' after if branch, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
            
            // else branch: could be a block OR another if expression
            let else_branch = if tokens.peek() == Some(&Token::If) {
                tokens.next();
                // Parse just the if part recursively
                parse_if_expr(tokens)?
            } else {
                parse_block(tokens)?
            };
            
            Ok(Expr::If(Box::new(cond), Box::new(then_branch), Box::new(else_branch)))
        }
        Some(t) => Err(format!("parse error: unexpected token: {:?}", t)),
        None => Err("parse error: unexpected end of input".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_only() {
        let tokens = vec![Token::Fn, Token::Ident("five".into()), Token::LParen, Token::RParen, 
            Token::LBrace, Token::Number(5), Token::RBrace, Token::Eof];
        let result = parse(tokens);
        assert!(result.is_ok());
    }
}
