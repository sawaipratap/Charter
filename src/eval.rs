//! Evaluator: run an expression and produce a value.
//!
//! Stage 3 implements effect handlers using continuation-passing style.

use crate::parse::{Expr, BinOp, Function, Program, HandlerClause};
use crate::{Value, Continuation};
use crate::lexer;
use crate::parse;
use std::collections::HashMap;
use std::cell::RefCell;

const RECURSION_LIMIT: usize = 10_000;

/// Thread-local flag to track if a continuation was invoked
thread_local! {
    static CONTINUATION_INVOKED: RefCell<bool> = RefCell::new(false);
    static IN_RESUMPTION: RefCell<bool> = RefCell::new(false);
    static HANDLER_FOUND: RefCell<bool> = RefCell::new(false);
}

/// A function value with its closure (captured environment).
#[derive(Debug, Clone)]
pub struct FunValue {
    pub func: Function,
    pub captured_env: HashMap<String, Value>,
}

/// Evaluate a complete program.
pub fn eval_program(program: Program) -> Result<Value, String> {
    // Build function registry
    let mut functions: HashMap<String, Function> = HashMap::new();
    for func in program.functions {
        functions.insert(func.name.clone(), func);
    }
    
    let env: HashMap<String, Value> = HashMap::new();
    
    eval_expr(&program.main, &env, &functions, 0, None)
}

/// Evaluate source string.
pub fn eval_string(source: &str) -> Result<Value, String> {
    let tokens = lexer::lex(source).map_err(|e| e.to_string())?;
    let ast = parse::parse(tokens).map_err(|e| e.to_string())?;
    eval_program(ast)
}

/// Evaluate an expression and return its value.
fn eval_expr(
    expr: &Expr,
    env: &HashMap<String, Value>,
    functions: &HashMap<String, Function>,
    depth: usize,
    handler: Option<&Handler>,
) -> Result<Value, String> {
    // Check recursion limit
    if depth > RECURSION_LIMIT {
        return Err("runtime error: recursion limit exceeded".to_string());
    }
    
    match expr {
        Expr::Number(n) => Ok(Value::Integer(*n)),
        
        Expr::Boolean(b) => Ok(Value::Bool(*b)),
        
        Expr::Ident(name) => {
            if name == "k" {
                return Err("runtime error: continuation 'k' used as value".to_string());
            }
            env.get(name)
                .cloned()
                .ok_or_else(|| format!("runtime error: unbound variable '{}'", name))
        }
        
        Expr::Block(bindings, final_expr) => {
            let mut new_env = env.clone();
            
            for (name, value_expr) in bindings {
                let value = eval_expr(value_expr, &new_env, functions, depth + 1, handler)?;
                new_env.insert(name.clone(), value);
            }
            
            eval_expr(final_expr, &new_env, functions, depth + 1, handler)
        }
        
        Expr::Call(name, args) => {
            // Check if 'k' is bound in the environment (continuation call)
            if name == "k" {
                if let Some(cont_val) = env.get("k") {
                    if let Value::Continuation(cont) = cont_val {
                        if args.len() != 1 {
                            return Err(format!("runtime error: continuation 'k' expects 1 argument, got {}", args.len()));
                        }
                        // Evaluate the argument
                        let arg_val = eval_expr(&args[0], env, functions, depth + 1, handler)?;
                        // Resume the continuation with the argument
                        return resume_continuation(cont, arg_val, handler);
                    }
                }
                return Err("runtime error: continuation 'k' called but not in scope".to_string());
            }
            
            // Normal function call
            let func = functions.get(name)
                .ok_or_else(|| format!("runtime error: undefined function '{}'", name))?;
            
            if args.len() != func.params.len() {
                return Err(format!("runtime error: wrong number of arguments to '{}' (expected {}, got {})", 
                    name, func.params.len(), args.len()));
            }
            
            let arg_values: Vec<Value> = args.iter()
                .map(|arg| eval_expr(arg, env, functions, depth + 1, handler))
                .collect::<Result<Vec<_>, _>>()?;
            
            let closure = env.clone();
            
            let fun_val = FunValue {
                func: func.clone(),
                captured_env: closure,
            };
            
            let mut call_env = fun_val.captured_env.clone();
            for (param, value) in func.params.iter().zip(arg_values) {
                call_env.insert(param.clone(), value);
            }
            
            // Evaluate function body - inherit the handler for lexical scoping
            eval_expr(&func.body, &call_env, functions, depth + 1, handler)
        }
        
        Expr::Perform(op_name, args) => {
            // Find the handler for this operation
            let h = handler.ok_or_else(|| 
                format!("runtime error: effect '{}' performed but not handled", op_name))?;
            
            // Find the matching clause
            let clause = h.clauses.iter()
                .find(|c| c.op_name == *op_name)
                .ok_or_else(|| 
                    format!("runtime error: no handler for effect '{}'", op_name))?;
            
            // Mark that a handler was found
            HANDLER_FOUND.with(|cell| {
                *cell.borrow_mut() = true;
            });
            
            // Evaluate the perform arguments
            let arg_vals: Vec<Value> = args.iter()
                .map(|arg| eval_expr(arg, env, functions, depth + 1, handler))
                .collect::<Result<Vec<_>, _>>()?;
            
            // Build the handler environment
            let mut handler_env = env.clone();
            
            // The clause's op_params are the operation's parameters (like 'p' in ask(p))
            for (param, val) in clause.op_params.iter().zip(arg_vals.iter()) {
                handler_env.insert(param.clone(), val.clone());
            }
            
            // Only create a placeholder continuation if k is not already bound
            if !handler_env.contains_key("k") {
                let cont = Continuation {
                    pending: Expr::Number(0),
                    env: env.clone(),
                    functions: functions.clone(),
                };
                handler_env.insert("k".to_string(), Value::Continuation(Box::new(cont)));
            }
            
            // Reset the continuation invoked flag for this handler invocation
            CONTINUATION_INVOKED.with(|cell| {
                *cell.borrow_mut() = false;
            });
            
            // Evaluate the clause body with the handler environment
            // Note: we pass the OUTER handler, not this handler
            let outer_handler = h.outer.as_ref().map(|h| h.as_ref());
            let result = eval_expr(&clause.body, &handler_env, functions, depth + 1, outer_handler);
            
            // Return the result - if a continuation was invoked, the result came from resume_continuation
            // The Binary case will check the flag
            result
        }
        
        Expr::Binary(left, op, right) => {
            // Check if we're in a resumption - if so, evaluate normally without capturing
            let in_resumption = IN_RESUMPTION.with(|cell| *cell.borrow());
            
            if in_resumption {
                // We're evaluating a continuation's pending expression - evaluate normally
                return eval_binary_normal(left, op, right, env, functions, depth, handler);
            }
            
            // Create the continuation: it <op> right
            let cont_expr = Expr::Binary(
                Box::new(Expr::Ident("_it".to_string())),
                op.clone(),
                right.clone(),
            );
            
            let cont = Continuation {
                pending: cont_expr,
                env: env.clone(),
                functions: functions.clone(),
            };
            
            // Build an environment with k bound to the continuation
            let mut cont_env = env.clone();
            cont_env.insert("k".to_string(), Value::Continuation(Box::new(cont)));
            
            // Reset the continuation invoked flag
            CONTINUATION_INVOKED.with(|cell| {
                *cell.borrow_mut() = false;
            });
            
            // Reset the handler found flag
            HANDLER_FOUND.with(|cell| {
                *cell.borrow_mut() = false;
            });
            
            // Evaluate left with the continuation in scope
            let left_result = eval_expr(left, &cont_env, functions, depth + 1, handler);
            
            // Check if a continuation was invoked
            let was_invoked = CONTINUATION_INVOKED.with(|cell| *cell.borrow());
            
            if was_invoked {
                // Continuation was invoked - return the result from resumption
                left_result
            } else {
                // No continuation was invoked
                // Check if a handler was found (perform happened) or not
                let handler_found = HANDLER_FOUND.with(|cell| *cell.borrow());
                
                if handler_found {
                    // Handler was found but k was not called (abort case)
                    // Return the handler's result
                    left_result
                } else {
                    // No handler found - left had no perform, evaluate normally
                    eval_binary_normal(left, op, right, &cont_env, functions, depth, handler)
                }
            }
        }
        
        Expr::UnaryMinus(inner) => {
            let val = eval_expr(inner, env, functions, depth + 1, handler)?;
            match val {
                Value::Integer(n) => Ok(Value::Integer(n.checked_neg()
                    .ok_or_else(|| "runtime error: integer overflow".to_string())?)),
                Value::Rational(num, den) => Ok(Value::Rational(-num, den)),
                Value::Bool(_) => Err("runtime error: unary minus applied to boolean".to_string()),
                Value::Continuation(_) => Err("runtime error: unary minus applied to continuation".to_string()),
            }
        }
        
        Expr::Not(inner) => {
            let val = eval_expr(inner, env, functions, depth + 1, handler)?;
            match val {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                Value::Continuation(_) => Err("runtime error: ! applied to continuation".to_string()),
                _ => Err("runtime error: ! applied to non-boolean".to_string()),
            }
        }
        
        Expr::If(cond, then_branch, else_branch) => {
            let cond_val = eval_expr(cond, env, functions, depth + 1, handler)?;
            match cond_val {
                Value::Bool(true) => eval_expr(then_branch, env, functions, depth + 1, handler),
                Value::Bool(false) => eval_expr(else_branch, env, functions, depth + 1, handler),
                Value::Continuation(_) => Err("runtime error: if condition is a continuation".to_string()),
                _ => Err("runtime error: if condition must be boolean".to_string()),
            }
        }
        
        Expr::Handle(body, clauses) => {
            // Create a handler with these clauses
            let h = Handler {
                clauses: clauses.clone(),
                outer: handler.map(|h| Box::new(h.clone())),
            };
            
            // Evaluate the body with this handler installed
            eval_expr(body, env, functions, depth + 1, Some(&h))
        }
    }
}

/// Evaluate a binary operation normally (no continuation capture)
fn eval_binary_normal(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    env: &HashMap<String, Value>,
    functions: &HashMap<String, Function>,
    depth: usize,
    handler: Option<&Handler>,
) -> Result<Value, String> {
    match op {
        BinOp::And => {
            let left_val = eval_expr(left, env, functions, depth + 1, handler)?;
            match left_val {
                Value::Bool(false) => Ok(Value::Bool(false)),
                Value::Bool(true) => {
                    let right_val = eval_expr(right, env, functions, depth + 1, handler)?;
                    match right_val {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        Value::Continuation(_) => Err("runtime error: and with continuation".to_string()),
                        _ => Err("runtime error: and applied to non-boolean".to_string()),
                    }
                }
                Value::Continuation(_) => Err("runtime error: and with continuation".to_string()),
                _ => Err("runtime error: and applied to non-boolean".to_string()),
            }
        }
        BinOp::Or => {
            let left_val = eval_expr(left, env, functions, depth + 1, handler)?;
            match left_val {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Bool(false) => {
                    let right_val = eval_expr(right, env, functions, depth + 1, handler)?;
                    match right_val {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        Value::Continuation(_) => Err("runtime error: or with continuation".to_string()),
                        _ => Err("runtime error: or applied to non-boolean".to_string()),
                    }
                }
                Value::Continuation(_) => Err("runtime error: or with continuation".to_string()),
                _ => Err("runtime error: or applied to non-boolean".to_string()),
            }
        }
        _ => {
            let left_val = eval_expr(left, env, functions, depth + 1, handler)?;
            let right_val = eval_expr(right, env, functions, depth + 1, handler)?;
            
            match op {
                BinOp::Add => left_val.add(right_val),
                BinOp::Sub => left_val.sub(right_val),
                BinOp::Mul => left_val.mul(right_val),
                BinOp::Div => left_val.div(right_val),
                BinOp::Less => compare_values(&left_val, &right_val, |a, b| a < b),
                BinOp::LessEqual => compare_values(&left_val, &right_val, |a, b| a <= b),
                BinOp::Greater => compare_values(&left_val, &right_val, |a, b| a > b),
                BinOp::GreaterEqual => compare_values(&left_val, &right_val, |a, b| a >= b),
                BinOp::Equal => equals_values(&left_val, &right_val),
                BinOp::NotEqual => {
                    let eq = equals_values(&left_val, &right_val)?;
                    match eq {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        _ => Err("internal error: equals_values returned non-bool".to_string()),
                    }
                }
                BinOp::And | BinOp::Or => unreachable!(),
            }
        }
    }
}

/// Resume execution with a continuation's value.
fn resume_continuation(cont: &Continuation, value: Value, handler: Option<&Handler>) -> Result<Value, String> {
    // Mark that the continuation was invoked
    CONTINUATION_INVOKED.with(|cell| {
        *cell.borrow_mut() = true;
    });
    
    // Mark that we're in a resumption
    IN_RESUMPTION.with(|cell| {
        *cell.borrow_mut() = true;
    });
    
    // Check if this is a placeholder continuation (no pending computation)
    // Placeholder continuations just return the value passed to k
    if let Expr::Number(0) = cont.pending {
        // This is a placeholder - return the value directly
        IN_RESUMPTION.with(|cell| {
            *cell.borrow_mut() = false;
        });
        return Ok(value);
    }
    
    // Create an environment with '_it' bound to the value
    let mut new_env = cont.env.clone();
    new_env.insert("_it".to_string(), value);
    
    // Evaluate the pending expression
    let result = eval_expr(&cont.pending, &new_env, &cont.functions, 0, handler);
    
    // Clear the resumption flag
    IN_RESUMPTION.with(|cell| {
        *cell.borrow_mut() = false;
    });
    
    result
}

/// A handler with clauses and an optional outer handler.
#[derive(Clone)]
pub struct Handler {
    pub clauses: Vec<HandlerClause>,
    pub outer: Option<Box<Handler>>,
}

fn compare_values<F>(left: &Value, right: &Value, cmp: F) -> Result<Value, String>
where
    F: FnOnce(f64, f64) -> bool,
{
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(cmp(*a as f64, *b as f64))),
        (Value::Integer(a), Value::Rational(bn, bd)) 
        | (Value::Rational(bn, bd), Value::Integer(a)) => {
            let a_rat = *a as f64;
            let b_rat = *bn as f64 / *bd as f64;
            Ok(Value::Bool(cmp(a_rat, b_rat)))
        }
        (Value::Rational(an, ad), Value::Rational(bn, bd)) => {
            let a_rat = *an as f64 / *ad as f64;
            let b_rat = *bn as f64 / *bd as f64;
            Ok(Value::Bool(cmp(a_rat, b_rat)))
        }
        (Value::Bool(_), _) | (_, Value::Bool(_)) => {
            Err("runtime error: cannot compare boolean values with <, <=, >, >=".to_string())
        }
        (Value::Continuation(_), _) | (_, Value::Continuation(_)) => {
            Err("runtime error: cannot use continuation in comparison".to_string())
        }
    }
}

fn equals_values(left: &Value, right: &Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a == b)),
        (Value::Integer(a), Value::Rational(bn, bd)) => {
            Ok(Value::Bool((*a as f64) == (*bn as f64 / *bd as f64)))
        }
        (Value::Rational(an, ad), Value::Integer(b)) => {
            Ok(Value::Bool((*an as f64 / *ad as f64) == (*b as f64)))
        }
        (Value::Rational(an, ad), Value::Rational(bn, bd)) => {
            Ok(Value::Bool(an * bd == bn * ad))
        }
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a == b)),
        (Value::Continuation(_), _) | (_, Value::Continuation(_)) => {
            Err("runtime error: cannot compare continuations".to_string())
        }
        _ => Err("runtime error: cannot compare values of different types".to_string()),
    }
}
