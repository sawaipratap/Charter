//! Parser: turn a token stream into an abstract syntax tree (AST).

use crate::lexer::Token;
use std::collections::{HashMap, HashSet};

/// An operation within an effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Operation {
    pub name: String,
    pub arity: usize,
}

/// An effect declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub name: String,
    pub ops: Vec<Operation>,
}

/// A function definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub uses: Vec<String>,  // effects declared in uses clause
    pub body: Expr,
}

/// A handler clause: operation name, operation params, continuation parameter, and handler body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerClause {
    pub op_name: String,
    pub op_params: Vec<String>,  // operation parameters (e.g., 'p' in ask(p))
    pub param: String,           // continuation parameter 'k'
    pub body: Expr,
}

/// A complete program: effect declarations, function definitions, and a main expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub effects: Vec<Effect>,
    pub functions: Vec<Function>,
    pub main: Expr,
}

/// An expression in Charter's Stage 2.
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
    
    // Perform an operation (Stage 2)
    Perform(String, Vec<Expr>),  // op name, args
    
    // Binary operations
    Binary(Box<Expr>, BinOp, Box<Expr>),
    
    // Unary operations
    UnaryMinus(Box<Expr>),
    Not(Box<Expr>),
    
    // If expression
    If(Box<Expr>, Box<Expr>, Box<Expr>), // condition, then, else
    
    // Handle expression (Stage 3)
    Handle(Box<Expr>, Vec<HandlerClause>),  // body, handler clauses
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    
    // Collections for effects, functions, and main expression
    let mut effects = Vec::new();
    let mut seen_effects: HashMap<String, ()> = HashMap::new();
    let mut seen_ops: HashSet<String> = HashSet::new();
    let mut functions = Vec::new();
    let mut seen_fn_names: HashMap<String, ()> = HashMap::new();
    let mut main_expr = None;
    
    loop {
        match tokens.peek() {
            Some(Token::Effect) => {
                let effect = parse_effect(&mut tokens, &mut seen_effects, &mut seen_ops)?;
                effects.push(effect);
            }
            Some(Token::Fn) => {
                let func = parse_fn(&mut tokens, &mut seen_fn_names)?;
                functions.push(func);
            }
            Some(Token::Eof) => {
                tokens.next();
                break;
            }
            Some(_) => {
                // Main expression (only once)
                if main_expr.is_some() {
                    return Err("parse error: multiple expressions at top level".to_string());
                }
                main_expr = Some(parse_expr(&mut tokens)?);
            }
            None => break,
        }
    }
    
    let main = main_expr.ok_or_else(|| "parse error: no expression found".to_string())?;
    
    // Build maps for static checks
    let effect_map: HashMap<String, usize> = effects.iter()
        .enumerate()
        .map(|(i, e)| (e.name.clone(), i))
        .collect();
    
    let op_map: HashMap<String, (String, usize)> = effects.iter()
        .flat_map(|e| e.ops.iter().map(|op| (op.name.clone(), (e.name.clone(), op.arity))))
        .collect();
    
    let fn_map: HashMap<String, usize> = functions.iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    
    // Static checks
    check_static(&main, &functions, &fn_map, &effect_map, &op_map, &HashSet::new())?;
    for func in &functions {
        // Function parameters are bound in the function body
        let mut param_set: HashSet<String> = HashSet::new();
        for param in &func.params {
            param_set.insert(param.clone());
        }
        check_static(&func.body, &functions, &fn_map, &effect_map, &op_map, &param_set)?;
    }
    
    // Effect checking - main expression should use no effects
    check_effects(&main, &functions, &fn_map, &effect_map, &op_map, &HashSet::new())?;
    for func in &functions {
        check_effects_fn(func, &functions, &fn_map, &effect_map, &op_map)?;
    }
    
    Ok(Program { effects, functions, main })
}

/// Parse an effect declaration: effect Name { op1(a, b), op2(), ... }
fn parse_effect(
    tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>, 
    seen_effects: &mut HashMap<String, ()>,
    seen_ops: &mut HashSet<String>,
) -> Result<Effect, String> {
    // Consume 'effect'
    match tokens.next() {
        Some(Token::Effect) => {}
        Some(t) => return Err(format!("parse error: expected 'effect', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Effect name
    let name = match tokens.next() {
        Some(Token::Ident(s)) => s,
        Some(t) => return Err(format!("parse error: expected effect name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    };
    
    if seen_effects.contains_key(&name) {
        return Err(format!("check error: duplicate effect name '{}'", name));
    }
    seen_effects.insert(name.clone(), ());
    
    // {
    match tokens.next() {
        Some(Token::LBrace) => {}
        Some(t) => return Err(format!("parse error: expected '{{' after effect name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Parse operations
    let mut ops = Vec::new();
    let mut first = true;
    
    while tokens.peek() != Some(&Token::RBrace) {
        if !first {
            // Expect comma
            match tokens.next() {
                Some(Token::Comma) => {}
                Some(Token::RBrace) => break,  // allow trailing comma
                Some(t) => return Err(format!("parse error: expected ',' between operations, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
        }
        first = false;
        
        // Operation name
        let op_name = match tokens.next() {
            Some(Token::Ident(s)) => s,
            Some(t) => return Err(format!("parse error: expected operation name, got {:?}", t)),
            None => return Err("parse error: unexpected end of input".to_string()),
        };
        
        if seen_ops.contains(&op_name) {
            return Err(format!("check error: duplicate operation name '{}'", op_name));
        }
        seen_ops.insert(op_name.clone());
        
        // (args)
        let arity = parse_param_list_opt(tokens)?;
        
        ops.push(Operation { name: op_name, arity });
    }
    
    // }
    match tokens.next() {
        Some(Token::RBrace) => {}
        Some(t) => return Err(format!("parse error: expected '}}' after effect operations, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    if ops.is_empty() {
        return Err("parse error: effect must have at least one operation".to_string());
    }
    
    Ok(Effect { name, ops })
}

/// Parse optional (a, b, c) parameter list, returning arity
fn parse_param_list_opt(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<usize, String> {
    if tokens.peek() != Some(&Token::LParen) {
        return Ok(0);  // no params
    }
    tokens.next();  // consume (
    
    // Check for empty params
    if tokens.peek() == Some(&Token::RParen) {
        tokens.next();
        return Ok(0);
    }
    
    let mut arity = 0;
    
    // Parse first parameter
    match tokens.next() {
        Some(Token::Ident(_)) => { arity = 1; }
        Some(Token::RParen) => return Ok(0),
        Some(t) => return Err(format!("parse error: expected parameter name, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    // Parse remaining parameters
    while tokens.peek() != Some(&Token::RParen) {
        // Expect comma
        match tokens.next() {
            Some(Token::Comma) => {}
            Some(t) => return Err(format!("parse error: expected ',' in params, got {:?}", t)),
            None => return Err("parse error: unexpected end of input".to_string()),
        }
        
        // Expect parameter name
        match tokens.next() {
            Some(Token::Ident(_)) => { arity += 1; }
            Some(Token::RParen) => return Err("parse error: trailing comma in params".to_string()),
            Some(t) => return Err(format!("parse error: expected parameter name, got {:?}", t)),
            None => return Err("parse error: unexpected end of input".to_string()),
        }
    }
    
    // Consume )
    match tokens.next() {
        Some(Token::RParen) => {}
        Some(t) => return Err(format!("parse error: expected ')' after params, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    Ok(arity)
}

/// Parse a function definition: fn name(params) uses [effects] { body }
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
    
    // Optional uses clause
    let mut uses = Vec::new();
    if tokens.peek() == Some(&Token::Uses) {
        tokens.next();  // consume 'uses'
        uses = parse_effect_list(tokens)?;
    }
    
    // Function body (a block)
    let body = parse_block(tokens)?;
    
    Ok(Function { name, params, uses, body })
}

/// Parse [Effect1, Effect2, ...]
fn parse_effect_list(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Vec<String>, String> {
    // [
    match tokens.next() {
        Some(Token::LBracket) => {}
        Some(t) => return Err(format!("parse error: expected '[' after 'uses', got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    let mut effects = Vec::new();
    let mut first = true;
    
    while tokens.peek() != Some(&Token::RBracket) {
        if !first {
            match tokens.next() {
                Some(Token::Comma) => {}
                Some(Token::RBracket) => break,
                Some(t) => return Err(format!("parse error: expected ',' in uses list, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
        }
        first = false;
        
        let name = match tokens.next() {
            Some(Token::Ident(s)) => s,
            Some(Token::RBracket) => break,
            Some(t) => return Err(format!("parse error: expected effect name, got {:?}", t)),
            None => return Err("parse error: unexpected end of input".to_string()),
        };
        effects.push(name);
        
        if tokens.peek() == Some(&Token::RBracket) {
            break;
        }
    }
    
    match tokens.next() {
        Some(Token::RBracket) => {}
        Some(t) => return Err(format!("parse error: expected ']' after uses list, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    Ok(effects)
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

fn parse_primary(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    match tokens.next() {
        Some(Token::Number(n)) => Ok(Expr::Number(n)),
        Some(Token::True) => Ok(Expr::Boolean(true)),
        Some(Token::False) => Ok(Expr::Boolean(false)),
        Some(Token::Ident(s)) => {
            // Check if it's a function call or perform
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
        Some(Token::Perform) => {
            // perform op(args)
            let op_name = match tokens.next() {
                Some(Token::Ident(s)) => s,
                Some(t) => return Err(format!("parse error: expected operation name after 'perform', got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            };
            
            // Parse args
            match tokens.next() {
                Some(Token::LParen) => {}
                None => return Err("parse error: unexpected end of input".to_string()),
                Some(t) => return Err(format!("parse error: expected '(' after perform op name, got {:?}", t)),
            }
            
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
                        Some(t) => return Err(format!("parse error: expected ',' or ')' in perform, got {:?}", t)),
                        None => return Err("parse error: unexpected end of input".to_string()),
                    }
                }
            }
            
            match tokens.next() {
                Some(Token::RParen) => Ok(Expr::Perform(op_name, args)),
                Some(t) => Err(format!("parse error: expected ')' after perform args, got {:?}", t)),
                None => Err("parse error: unexpected end of input".to_string()),
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
            let cond = parse_expr(tokens)?;
            let then_branch = parse_block(tokens)?;
            
            match tokens.next() {
                Some(Token::Else) => {}
                Some(t) => return Err(format!("parse error: expected 'else' after if branch, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
            
            let else_branch = if tokens.peek() == Some(&Token::If) {
                tokens.next();
                parse_if_expr(tokens)?
            } else {
                parse_block(tokens)?
            };
            
            Ok(Expr::If(Box::new(cond), Box::new(then_branch), Box::new(else_branch)))
        }
        Some(Token::Handle) => {
            // handle { block } with { op(args) k => expr, ... }
            let body = parse_block(tokens)?;
            
            // 'with'
            match tokens.next() {
                Some(Token::With) => {}
                Some(t) => return Err(format!("parse error: expected 'with' after handle block, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
            
            // '{'
            match tokens.next() {
                Some(Token::LBrace) => {}
                Some(t) => return Err(format!("parse error: expected '{{' after 'with', got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
            
            // Parse handler clauses
            let mut clauses = Vec::new();
            let mut first = true;
            loop {
                // Check for empty handler (no clauses) - already handled at the end
                // Check for end of clauses
                if tokens.peek() == Some(&Token::RBrace) {
                    break;
                }
                
                if !first {
                    // Expect comma, then either another clause or end
                    match tokens.next() {
                        Some(Token::Comma) => {
                            // Check if there's a clause after the comma or just '}'
                            if tokens.peek() == Some(&Token::RBrace) {
                                // Trailing comma - that's OK, break
                                break;
                            }
                        }
                        Some(Token::RBrace) => break,
                        Some(t) => return Err(format!("parse error: expected ',' between handler clauses, got {:?}", t)),
                        None => return Err("parse error: unexpected end of input".to_string()),
                    }
                }
                first = false;
                
                // Parse clause: op(args) k => expr
                let op_name = match tokens.next() {
                    Some(Token::Ident(s)) => s,
                    Some(t) => return Err(format!("parse error: expected operation name in handler clause, got {:?}", t)),
                    None => return Err("parse error: unexpected end of input".to_string()),
                };
                
                // Parse args in parens
                match tokens.next() {
                    Some(Token::LParen) => {}
                    None => return Err("parse error: unexpected end of input".to_string()),
                    Some(t) => return Err(format!("parse error: expected '(' after op name in handler clause, got {:?}", t)),
                }
                
                // Parse op args (may be multiple)
                let mut args = Vec::new();
                let mut first_arg = true;
                while tokens.peek() != Some(&Token::RParen) {
                    if !first_arg {
                        match tokens.next() {
                            Some(Token::Comma) => {}
                            Some(t) => return Err(format!("parse error: expected ',' in op args, got {:?}", t)),
                            None => return Err("parse error: unexpected end of input".to_string()),
                        }
                    }
                    first_arg = false;
                    
                    match tokens.next() {
                        Some(Token::Ident(s)) => args.push(s),
                        Some(Token::RParen) => break,
                        Some(t) => return Err(format!("parse error: expected arg name or ')' in handler clause, got {:?}", t)),
                        None => return Err("parse error: unexpected end of input".to_string()),
                    }
                }
                
                match tokens.next() {
                    Some(Token::RParen) => {}
                    Some(t) => return Err(format!("parse error: expected ')' after op args in handler clause, got {:?}", t)),
                    None => return Err("parse error: unexpected end of input".to_string()),
                }
                
                // The continuation parameter 'k'
                let k_param = match tokens.next() {
                    Some(Token::Ident(s)) => s,
                    Some(t) => return Err(format!("parse error: expected continuation parameter 'k' in handler clause, got {:?}", t)),
                    None => return Err("parse error: unexpected end of input".to_string()),
                };
                
                // '=>'
                match tokens.next() {
                    Some(Token::Arrow) => {}
                    Some(t) => return Err(format!("parse error: expected '=>' in handler clause, got {:?}", t)),
                    None => return Err("parse error: unexpected end of input".to_string()),
                }
                
                // Handler body expression
                let clause_body = parse_expr(tokens)?;
                
                // Store the handler clause with op params
                clauses.push(HandlerClause {
                    op_name,
                    op_params: args,
                    param: k_param,
                    body: clause_body,
                });
            }
            
            // '}'
            match tokens.next() {
                Some(Token::RBrace) => {}
                Some(t) => return Err(format!("parse error: expected '}}' after handler clauses, got {:?}", t)),
                None => return Err("parse error: unexpected end of input".to_string()),
            }
            
            // Require at least one clause
            if clauses.is_empty() {
                return Err("parse error: handler must have at least one clause".to_string());
            }
            
            Ok(Expr::Handle(Box::new(body), clauses))
        }
        Some(t) => Err(format!("parse error: unexpected token: {:?}", t)),
        None => Err("parse error: unexpected end of input".to_string()),
    }
}

/// Parse an if expression without the leading 'if' token.
fn parse_if_expr(tokens: &mut std::iter::Peekable<impl Iterator<Item = Token>>) -> Result<Expr, String> {
    let cond = parse_expr(tokens)?;
    let then_branch = parse_block(tokens)?;
    
    match tokens.next() {
        Some(Token::Else) => {}
        Some(t) => return Err(format!("parse error: expected 'else' after if branch, got {:?}", t)),
        None => return Err("parse error: unexpected end of input".to_string()),
    }
    
    let else_branch = if tokens.peek() == Some(&Token::If) {
        tokens.next();
        parse_if_expr(tokens)?
    } else {
        parse_block(tokens)?
    };
    
    Ok(Expr::If(Box::new(cond), Box::new(then_branch), Box::new(else_branch)))
}

// === Static checking ===

/// Static checks: function references and variable scope
fn check_static(
    expr: &Expr, 
    functions: &[Function],
    fn_map: &HashMap<String, usize>,
    effect_map: &HashMap<String, usize>,
    op_map: &HashMap<String, (String, usize)>,
    bound_vars: &HashSet<String>,
) -> Result<HashSet<String>, String> {
    match expr {
        Expr::Number(_) | Expr::Boolean(_) => Ok(bound_vars.clone()),
        Expr::Ident(name) => {
            if !bound_vars.contains(name) {
                return Err(format!("check error: unbound variable '{}'", name));
            }
            Ok(bound_vars.clone())
        }
        Expr::Block(bindings, body) => {
            let mut new_bound = bound_vars.clone();
            for (name, e) in bindings {
                check_static(e, functions, fn_map, effect_map, op_map, &new_bound)?;
                new_bound.insert(name.clone());
            }
            check_static(body, functions, fn_map, effect_map, op_map, &new_bound)
        }
        Expr::Call(name, args) => {
            // Check if it's a continuation call (the continuation parameter 'k')
            // Continuation calls are only allowed if 'k' is in scope
            let is_continuation = name == "k" && bound_vars.contains("k");
            
            if !fn_map.contains_key(name) && !is_continuation {
                return Err(format!("check error: undefined function '{}'", name));
            }
            // Check arity
            if let Some(&idx) = fn_map.get(name) {
                let func = &functions[idx];
                if args.len() != func.params.len() {
                    return Err(format!("check error: wrong number of arguments to '{}' (expected {}, got {})", 
                        name, func.params.len(), args.len()));
                }
            }
            let mut new_bound = bound_vars.clone();
            for arg in args {
                new_bound = check_static(arg, functions, fn_map, effect_map, op_map, &new_bound)?;
            }
            Ok(new_bound)
        }
        Expr::Perform(name, args) => {
            // Check operation exists and arity
            if let Some((_, expected_arity)) = op_map.get(name) {
                if args.len() != *expected_arity {
                    return Err(format!("check error: wrong number of arguments to '{}' (expected {}, got {})", 
                        name, expected_arity, args.len()));
                }
            } else {
                return Err(format!("check error: unknown operation '{}'", name));
            }
            let mut new_bound = bound_vars.clone();
            for arg in args {
                new_bound = check_static(arg, functions, fn_map, effect_map, op_map, &new_bound)?;
            }
            Ok(new_bound)
        }
        Expr::Binary(left, _, right) => {
            let new_bound = check_static(left, functions, fn_map, effect_map, op_map, bound_vars)?;
            check_static(right, functions, fn_map, effect_map, op_map, &new_bound)
        }
        Expr::UnaryMinus(inner) | Expr::Not(inner) => {
            check_static(inner, functions, fn_map, effect_map, op_map, bound_vars)
        }
        Expr::If(cond, then, else_) => {
            let new_bound = check_static(cond, functions, fn_map, effect_map, op_map, bound_vars)?;
            let new_bound = check_static(then, functions, fn_map, effect_map, op_map, &new_bound)?;
            check_static(else_, functions, fn_map, effect_map, op_map, &new_bound)
        }
        Expr::Handle(body, clauses) => {
            // Handler body can see the outer scope
            let mut new_bound = bound_vars.clone();
            let mut seen_ops: HashSet<String> = HashSet::new();
            
            // Check each clause - the clause body can see the continuation param, op params, and outer scope
            for clause in clauses {
                // Check for duplicate clauses
                if seen_ops.contains(&clause.op_name) {
                    return Err(format!("check error: handler for '{}' defined multiple times", clause.op_name));
                }
                seen_ops.insert(clause.op_name.clone());
                
                // Check handler clause arity matches operation arity
                if let Some((_, expected_arity)) = op_map.get(&clause.op_name) {
                    if clause.op_params.len() != *expected_arity {
                        return Err(format!("check error: handler for '{}' has {} parameters but operation expects {}",
                            clause.op_name, clause.op_params.len(), expected_arity));
                    }
                } else {
                    return Err(format!("check error: handler for unknown operation '{}'", clause.op_name));
                }
                
                // Add the op params to scope (e.g., 'p' in ask(p))
                for param in &clause.op_params {
                    new_bound.insert(param.clone());
                }
                // Add the continuation parameter to scope
                new_bound.insert(clause.param.clone());
                // Check the clause body
                new_bound = check_static(&clause.body, functions, fn_map, effect_map, op_map, &new_bound)?;
            }
            
            // Check the handler body
            check_static(body, functions, fn_map, effect_map, op_map, bound_vars)
        }
    }
}

// === Effect checking ===

/// Check that all effects used in an expression are declared in uses
fn check_effects(
    expr: &Expr,
    functions: &[Function],
    fn_map: &HashMap<String, usize>,
    effect_map: &HashMap<String, usize>,
    op_map: &HashMap<String, (String, usize)>,
    uses: &HashSet<String>,
) -> Result<(), String> {
    // Collect effects used in this expression (transitively through function calls)
    let mut used_effects: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_all_effects(expr, functions, fn_map, op_map, &mut used_effects, &mut visited)?;
    
    // Check all used effects are in uses
    for eff in &used_effects {
        if !uses.contains(eff) {
            return Err(format!("effect error: effect '{}' used but not declared", eff));
        }
    }
    
    Ok(())
}

/// Check effects for a function (including transitive function calls)
fn check_effects_fn(
    func: &Function,
    functions: &[Function],
    fn_map: &HashMap<String, usize>,
    effect_map: &HashMap<String, usize>,
    op_map: &HashMap<String, (String, usize)>,
) -> Result<(), String> {
    // Get the uses set for this function
    let uses: HashSet<String> = func.uses.iter().cloned().collect();
    
    // Check uses are declared FIRST (before checking used effects)
    for eff in &func.uses {
        if !effect_map.contains_key(eff) {
            return Err(format!("check error: undeclared effect '{}' in uses clause", eff));
        }
    }
    
    // Collect all effects used (direct + transitive)
    let mut used_effects: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_all_effects(&func.body, functions, fn_map, op_map, &mut used_effects, &mut visited)?;
    
    // Check all used effects are in uses
    for eff in &used_effects {
        if !uses.contains(eff) {
            return Err(format!("effect error: effect '{}' used but not declared in '{}'", eff, func.name));
        }
    }
    
    // Check for duplicate uses
    let mut seen: HashSet<String> = HashSet::new();
    for eff in &func.uses {
        if !seen.insert(eff.clone()) {
            return Err(format!("check error: duplicate effect '{}' in uses clause", eff));
        }
    }
    
    Ok(())
}

/// Collect all effects through function calls (transitively)
fn collect_all_effects(
    expr: &Expr,
    functions: &[Function],
    fn_map: &HashMap<String, usize>,
    op_map: &HashMap<String, (String, usize)>,
    used: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> Result<(), String> {
    match expr {
        Expr::Number(_) | Expr::Boolean(_) | Expr::Ident(_) => Ok(()),
        Expr::Perform(op_name, _args) => {
            if let Some((eff_name, _)) = op_map.get(op_name) {
                used.insert(eff_name.clone());
            } else {
                return Err(format!("check error: unknown operation '{}'", op_name));
            }
            Ok(())
        }
        Expr::Block(bindings, body) => {
            for (_, e) in bindings {
                collect_all_effects(e, functions, fn_map, op_map, used, visited)?;
            }
            collect_all_effects(body, functions, fn_map, op_map, used, visited)
        }
        Expr::Call(name, args) => {
            // First, collect effects from the arguments
            for arg in args {
                collect_all_effects(arg, functions, fn_map, op_map, used, visited)?;
            }
            
            // Get the called function and collect its effects
            if let Some(&idx) = fn_map.get(name) {
                if !visited.insert(name.clone()) {
                    // Already visited this function, avoid infinite loop
                    return Ok(());
                }
                let called_func = &functions[idx];
                // Collect effects from the called function's body
                collect_all_effects(&called_func.body, functions, fn_map, op_map, used, visited)?;
                // Add the effects declared in the called function's uses clause
                for eff in &called_func.uses {
                    used.insert(eff.clone());
                }
            }
            Ok(())
        }
        Expr::Binary(left, _, right) => {
            collect_all_effects(left, functions, fn_map, op_map, used, visited)?;
            collect_all_effects(right, functions, fn_map, op_map, used, visited)
        }
        Expr::UnaryMinus(inner) | Expr::Not(inner) => {
            collect_all_effects(inner, functions, fn_map, op_map, used, visited)
        }
        Expr::If(cond, then, else_) => {
            collect_all_effects(cond, functions, fn_map, op_map, used, visited)?;
            collect_all_effects(then, functions, fn_map, op_map, used, visited)?;
            collect_all_effects(else_, functions, fn_map, op_map, used, visited)
        }
        Expr::Handle(body, _clauses) => {
            // Effects inside a Handle are DISCHARGED by the handler
            // So we do NOT collect effects from the body or clauses
            // The handler is responsible for handling them
            Ok(())
        }
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
