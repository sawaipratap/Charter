//! Evaluator: run an expression and produce a value.

use crate::parse::{Expr, BinOp, Function, Program};
use crate::Value;
use std::collections::HashMap;

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
    
    // In Stage 2, effects are checked but not executed
    // The main expression should not use any effects (nothing handles them)
    let env: HashMap<String, Value> = HashMap::new();
    let fn_env: HashMap<String, FunValue> = HashMap::new();
    
    eval_expr(&program.main, &env, &functions, &fn_env)
}

/// Evaluate an expression and return its value.
fn eval_expr(
    expr: &Expr,
    env: &HashMap<String, Value>,
    functions: &HashMap<String, Function>,
    fn_env: &HashMap<String, FunValue>,
) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Integer(*n)),
        
        Expr::Boolean(b) => Ok(Value::Bool(*b)),
        
        Expr::Ident(name) => {
            env.get(name)
                .cloned()
                .ok_or_else(|| format!("runtime error: unbound variable '{}'", name))
        }
        
        Expr::Block(bindings, final_expr) => {
            let mut new_env = env.clone();
            
            for (name, value_expr) in bindings {
                let value = eval_expr(value_expr, &new_env, functions, fn_env)?;
                new_env.insert(name.clone(), value);
            }
            
            eval_expr(final_expr, &new_env, functions, fn_env)
        }
        
        Expr::Call(name, args) => {
            let func = functions.get(name)
                .ok_or_else(|| format!("runtime error: undefined function '{}'", name))?;
            
            if args.len() != func.params.len() {
                return Err(format!("runtime error: wrong number of arguments to '{}' (expected {}, got {})", 
                    name, func.params.len(), args.len()));
            }
            
            let arg_values: Vec<Value> = args.iter()
                .map(|arg| eval_expr(arg, env, functions, fn_env))
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
            
            // Evaluate function body
            eval_expr(&func.body, &call_env, functions, fn_env)
        }
        
        Expr::Perform(op_name, args) => {
            // In Stage 2, perform doesn't execute - effects are only checked
            // This should never be reached for valid programs (they're rejected before execution)
            Err(format!("runtime error: effect '{}' performed but not handled", op_name))
        }
        
        Expr::Binary(left, op, right) => {
            eval_binary(left, op, right, env, functions, fn_env)
        }
        
        Expr::UnaryMinus(inner) => {
            let val = eval_expr(inner, env, functions, fn_env)?;
            match val {
                Value::Integer(n) => Ok(Value::Integer(n.checked_neg()
                    .ok_or_else(|| "runtime error: integer overflow".to_string())?)),
                Value::Rational(num, den) => Ok(Value::Rational(-num, den)),
                Value::Bool(_) => Err("runtime error: unary minus applied to boolean".to_string()),
            }
        }
        
        Expr::Not(inner) => {
            let val = eval_expr(inner, env, functions, fn_env)?;
            match val {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err("runtime error: ! applied to non-boolean".to_string()),
            }
        }
        
        Expr::If(cond, then_branch, else_branch) => {
            let cond_val = eval_expr(cond, env, functions, fn_env)?;
            match cond_val {
                Value::Bool(true) => eval_expr(then_branch, env, functions, fn_env),
                Value::Bool(false) => eval_expr(else_branch, env, functions, fn_env),
                _ => Err("runtime error: if condition must be boolean".to_string()),
            }
        }
    }
}

fn eval_binary(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    env: &HashMap<String, Value>,
    functions: &HashMap<String, Function>,
    fn_env: &HashMap<String, FunValue>,
) -> Result<Value, String> {
    match op {
        BinOp::And => {
            let left_val = eval_expr(left, env, functions, fn_env)?;
            match left_val {
                Value::Bool(false) => Ok(Value::Bool(false)),
                Value::Bool(true) => {
                    let right_val = eval_expr(right, env, functions, fn_env)?;
                    match right_val {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        _ => Err("runtime error: and applied to non-boolean".to_string()),
                    }
                }
                _ => Err("runtime error: and applied to non-boolean".to_string()),
            }
        }
        BinOp::Or => {
            let left_val = eval_expr(left, env, functions, fn_env)?;
            match left_val {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Bool(false) => {
                    let right_val = eval_expr(right, env, functions, fn_env)?;
                    match right_val {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        _ => Err("runtime error: or applied to non-boolean".to_string()),
                    }
                }
                _ => Err("runtime error: or applied to non-boolean".to_string()),
            }
        }
        _ => {
            let left_val = eval_expr(left, env, functions, fn_env)?;
            let right_val = eval_expr(right, env, functions, fn_env)?;
            
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
        (Value::Bool(_), _) | (_, Value::Bool(_)) => {
            Err("runtime error: cannot compare boolean with number".to_string())
        }
    }
}
