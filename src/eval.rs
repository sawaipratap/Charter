//! Stack machine evaluator with defunctionalized continuations.
//!
//! Stage 3 implements effect handlers using a stack machine.

use crate::parse::{Expr, BinOp, Function, Program, HandlerClause};
use crate::lexer;
use crate::parse;
use std::collections::HashMap;
use std::rc::Rc;

/// Frame limit for recursion depth
const FRAME_LIMIT: usize = 10_000;

/// An immutable, shareable environment using reference counting.
#[derive(Clone, PartialEq, Eq)]
pub struct Env {
    /// The parent environment (for lexical scoping)
    parent: Option<Rc<Env>>,
    /// Bindings in this environment
    bindings: HashMap<String, Value>,
}

impl Env {
    fn new() -> Self {
        Env {
            parent: None,
            bindings: HashMap::new(),
        }
    }
    
    fn with_parent(parent: Rc<Env>) -> Self {
        Env {
            parent: Some(parent),
            bindings: HashMap::new(),
        }
    }
    
    fn extend(&self) -> Rc<Env> {
        Rc::new(Env::with_parent(Rc::new(self.clone())))
    }
    
    fn get(&self, name: &str) -> Option<Value> {
        self.bindings.get(name).cloned()
            .or_else(|| self.parent.as_ref().and_then(|p| p.get(name)))
    }
    
    fn insert(&self, name: String, value: Value) -> Rc<Env> {
        let mut new_env = (*self).clone();
        new_env.bindings.insert(name, value);
        Rc::new(new_env)
    }
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Env")
    }
}

/// A continuation is a cloneable stack of frames.
/// Stored as [HandleScope, f_bottom, ..., f_top]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Continuation {
    pub frames: Vec<Frame>,
}

/// A value in Charter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Integer(i64),
    Rational(i64, i64),
    Bool(bool),
    Continuation(Continuation),
}

/// A frame on the evaluation stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    BinLeft { op: BinOp, right: Expr, env: Rc<Env> },
    BinRight { op: BinOp, left_val: Value },
    Let { name: String, body: Expr, env: Rc<Env> },
    CallArgs { name: String, params: Vec<String>, args: Vec<Expr>, done: Vec<Value>, env: Rc<Env> },
    CallBody { name: String, params: Vec<String>, args_vals: Vec<Value>, closure_env: Rc<Env> },
    If { then_branch: Expr, else_branch: Expr, env: Rc<Env> },
    Block { bindings: Vec<(String, Expr)>, remaining: Vec<(String, Expr)>, body: Expr, env: Rc<Env> },
    UnaryMinus,
    Not,
    HandleScope { clauses: Vec<HandlerClause> },
    PerformArgs { op: String, args: Vec<Expr>, done: Vec<Value>, env: Rc<Env> },
    KCallArg { cont: Continuation },
}

/// State of the stack machine.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Eval(Expr, Rc<Env>),
    Return(Value),
}

/// Evaluate a complete program.
pub fn eval_program(program: Program) -> Result<Value, String> {
    let mut functions: HashMap<String, Function> = HashMap::new();
    for func in program.functions {
        functions.insert(func.name.clone(), func);
    }
    
    let env = Rc::new(Env::new());
    
    run_machine(&program.main, env, &functions)
}

/// Evaluate source string.
pub fn eval_string(source: &str) -> Result<Value, String> {
    let tokens = lexer::lex(source).map_err(|e| e.to_string())?;
    let ast = parse::parse(tokens).map_err(|e| e.to_string())?;
    eval_program(ast)
}

/// Run the stack machine iteratively.
fn run_machine(expr: &Expr, env: Rc<Env>, functions: &HashMap<String, Function>) -> Result<Value, String> {
    let mut state = State::Eval(expr.clone(), env);
    let mut stack: Vec<Frame> = Vec::new();
    let mut call_depth: usize = 0;
    
    loop {
        match &state {
            State::Eval(e, env) => {
                match e {
                    Expr::Number(n) => {
                        state = State::Return(Value::Integer(*n));
                    }
                    Expr::Boolean(b) => {
                        state = State::Return(Value::Bool(*b));
                    }
                    Expr::Ident(name) => {
                        if name == "k" {
                            return Err("runtime error: continuation 'k' used as value".to_string());
                        }
                        match env.get(name) {
                            Some(v) => state = State::Return(v),
                            None => return Err(format!("runtime error: unbound variable '{}'", name)),
                        }
                    }
                    Expr::Block(bindings, body) => {
                        if bindings.is_empty() {
                            state = State::Eval((**body).clone(), env.clone());
                        } else {
                            // Push Block frame and evaluate first binding's expression
                            stack.push(Frame::Block {
                                bindings: bindings.clone(),
                                remaining: bindings.clone(),
                                body: (**body).clone(),
                                env: env.clone(),
                            });
                            state = State::Eval(bindings[0].1.clone(), env.clone());
                        }
                    }
                    Expr::Call(name, args) => {
                        if name == "k" {
                            if args.len() != 1 {
                                return Err(format!("runtime error: continuation 'k' expects 1 argument, got {}", args.len()));
                            }
                            // Look up k in the environment - it's a Value::Continuation
                            let k_value = match env.get(name) {
                                Some(v) => v.clone(),
                                None => return Err(format!("runtime error: undefined variable '{}'", name)),
                            };
                            let cont = match k_value {
                                Value::Continuation(c) => c,
                                _ => return Err(format!("runtime error: '{}' is not a continuation", name)),
                            };
                            stack.push(Frame::KCallArg { cont });
                            state = State::Eval(args[0].clone(), env.clone());
                        } else {
                            let func = match functions.get(name) {
                                Some(f) => f,
                                None => return Err(format!("runtime error: undefined function '{}'", name)),
                            };
                            if args.len() != func.params.len() {
                                return Err(format!("runtime error: wrong number of arguments to '{}' (expected {}, got {})",
                                    name, func.params.len(), args.len()));
                            }
                            if args.is_empty() {
                                // No arguments, call directly - evaluate body with calling env
                                // Params are empty so nothing to bind
                                state = State::Eval(func.body.clone(), env.clone());
                            } else {
                                stack.push(Frame::CallArgs {
                                    name: name.clone(),
                                    params: func.params.clone(),
                                    args: args.clone(),
                                    done: Vec::new(),
                                    env: env.clone(),
                                });
                                state = State::Eval(args[0].clone(), env.clone());
                            }
                        }
                    }
                    Expr::Perform(op_name, args) => {
                        stack.push(Frame::PerformArgs {
                            op: op_name.clone(),
                            args: args.clone(),
                            done: Vec::new(),
                            env: env.clone(),
                        });
                        if args.is_empty() {
                            // Zero-argument perform: trigger handler lookup immediately
                            // by simulating the args-done condition
                            let value = Value::Integer(0); // Dummy, ignored for zero args
                            // Pop the frame we just pushed and handle it
                            let frame = stack.pop().unwrap();
                            if let Frame::PerformArgs { op, args, done, env } = frame {
                                // Find handler and capture continuation
                                let scope_result = stack.iter().rposition(|f| {
                                    if let Frame::HandleScope { clauses } = f {
                                        clauses.iter().any(|c| c.op_name == op)
                                    } else {
                                        false
                                    }
                                });
                                match scope_result {
                                    Some(idx) => {
                                        let clauses = if let Frame::HandleScope { clauses } = &stack[idx] {
                                            clauses.clone()
                                        } else {
                                            unreachable!();
                                        };
                                        
                                        let clause = match clauses.iter().find(|c| c.op_name == op) {
                                            Some(c) => c,
                                            None => unreachable!(),
                                        };
                                        
                                        // Capture: frames ABOVE the HandleScope
                                        let above: Vec<Frame> = stack[idx+1..].to_vec();
                                        let mut captured: Vec<Frame> = vec![];
                                        captured.push(Frame::HandleScope { clauses: clauses.clone() });
                                        captured.extend(above);
                                        
                                        let k = Value::Continuation(Continuation { frames: captured });
                                        
                                        // Build handler env
                                        let mut handler_env = Env::new();
                                        for (param, val) in clause.op_params.iter().zip(done.iter()) {
                                            handler_env.bindings.insert(param.clone(), val.clone());
                                        }
                                        handler_env.bindings.insert("k".to_string(), k);
                                        
                                        // Remove HandleScope and above
                                        stack.truncate(idx);
                                        
                                        state = State::Eval(clause.body.clone(), Rc::new(handler_env));
                                    }
                                    None => {
                                        return Err(format!("runtime error: effect '{}' performed but not handled", op));
                                    }
                                }
                            } else {
                                unreachable!();
                            }
                        } else {
                            state = State::Eval(args[0].clone(), env.clone());
                        }
                    }
                    Expr::Binary(left, op, right) => {
                        stack.push(Frame::BinLeft {
                            op: op.clone(),
                            right: (**right).clone(),
                            env: env.clone(),
                        });
                        state = State::Eval((**left).clone(), env.clone());
                    }
                    Expr::UnaryMinus(inner) => {
                        stack.push(Frame::UnaryMinus);
                        state = State::Eval((**inner).clone(), env.clone());
                    }
                    Expr::Not(inner) => {
                        stack.push(Frame::Not);
                        state = State::Eval((**inner).clone(), env.clone());
                    }
                    Expr::If(cond, then_branch, else_branch) => {
                        stack.push(Frame::If {
                            then_branch: (**then_branch).clone(),
                            else_branch: (**else_branch).clone(),
                            env: env.clone(),
                        });
                        state = State::Eval((**cond).clone(), env.clone());
                    }
                    Expr::Handle(body, clauses) => {
                        stack.push(Frame::HandleScope {
                            clauses: clauses.clone(),
                        });
                        state = State::Eval((**body).clone(), env.clone());
                    }
                }
            }
            State::Return(value) => {
                if let Some(frame) = stack.pop() {
                    match frame {
                        Frame::BinLeft { op, right, env } => {
                            // Short-circuit evaluation for And/Or
                            match &op {
                                BinOp::And => {
                                    // If left is false, short-circuit and return false
                                    match &value {
                                        Value::Bool(false) => {
                                            state = State::Return(Value::Bool(false));
                                        }
                                        Value::Bool(true) => {
                                            // Evaluate right operand
                                            stack.push(Frame::BinRight { op: BinOp::And, left_val: value.clone() });
                                            state = State::Eval(right.clone(), env);
                                        }
                                        _ => return Err("runtime error: && requires boolean operands".to_string()),
                                    }
                                }
                                BinOp::Or => {
                                    // If left is true, short-circuit and return true
                                    match &value {
                                        Value::Bool(true) => {
                                            state = State::Return(Value::Bool(true));
                                        }
                                        Value::Bool(false) => {
                                            // Evaluate right operand
                                            stack.push(Frame::BinRight { op: BinOp::Or, left_val: value.clone() });
                                            state = State::Eval(right.clone(), env);
                                        }
                                        _ => return Err("runtime error: || requires boolean operands".to_string()),
                                    }
                                }
                                _ => {
                                    stack.push(Frame::BinRight { op, left_val: value.clone() });
                                    state = State::Eval(right.clone(), env);
                                }
                            }
                        }
                        Frame::BinRight { op, left_val } => {
                            // Handle And/Or for when we evaluated both operands
                            match &op {
                                BinOp::And => {
                                    match (&left_val, &value) {
                                        (Value::Bool(a), Value::Bool(b)) => {
                                            state = State::Return(Value::Bool(*a && *b));
                                        }
                                        _ => return Err("runtime error: && requires boolean operands".to_string()),
                                    }
                                }
                                BinOp::Or => {
                                    match (&left_val, &value) {
                                        (Value::Bool(a), Value::Bool(b)) => {
                                            state = State::Return(Value::Bool(*a || *b));
                                        }
                                        _ => return Err("runtime error: || requires boolean operands".to_string()),
                                    }
                                }
                                _ => {
                                    state = State::Return(apply_binop(&op, left_val.clone(), value.clone())?);
                                }
                            }
                        }
                        Frame::Let { name, body, env } => {
                            let new_env = env.insert(name, value.clone());
                            state = State::Eval(body, new_env);
                        }
                        Frame::CallArgs { name, params, args, mut done, env } => {
                            done.push(value.clone());
                            if done.len() == args.len() {
                                let func = match functions.get(&name) {
                                    Some(f) => f,
                                    None => return Err(format!("runtime error: undefined function '{}'", name)),
                                };
                                call_depth += 1;
                                if call_depth > FRAME_LIMIT {
                                    return Err("runtime error: recursion limit exceeded".to_string());
                                }
                                // Create call environment with parameters bound
                                let mut call_env = (*env).clone();
                                for (param, val) in params.iter().zip(done.iter()) {
                                    call_env.bindings.insert(param.clone(), val.clone());
                                }
                                let call_env_rc = Rc::new(call_env);
                                stack.push(Frame::CallBody {
                                    name: name.clone(),
                                    params: params.clone(),
                                    args_vals: done,
                                    closure_env: call_env_rc.clone(),
                                });
                                state = State::Eval(func.body.clone(), call_env_rc);
                            } else {
                                let next_idx = done.len();
                                stack.push(Frame::CallArgs {
                                    name: name.clone(),
                                    params: params.clone(),
                                    args: args.clone(),
                                    done,
                                    env: env.clone(),
                                });
                                state = State::Eval(args[next_idx].clone(), env);
                            }
                        }
                        Frame::CallBody { name: _, params: _, args_vals: _, closure_env } => {
                            call_depth = call_depth.saturating_sub(1);
                            // The call environment was already set up in CallArgs
                            // We just need to return from the function
                            // The return value is in 'value', so we just return it
                            // Actually, we don't need to do anything here - the function body
                            // has already been evaluated with the correct environment
                            // This frame is just a marker for call depth management
                            state = State::Return(value.clone());
                        }
                        Frame::If { then_branch, else_branch, env } => {
                            match value {
                                Value::Bool(true) => state = State::Eval(then_branch, env),
                                Value::Bool(false) => state = State::Eval(else_branch, env),
                                _ => return Err("runtime error: if condition must be boolean".to_string()),
                            }
                        }
                        Frame::Block { bindings, remaining, body, env } => {
                            // We've just evaluated the expression for remaining[0]
                            // Now we need to bind it and either continue with next binding or evaluate body
                            let (name, _) = remaining[0].clone();
                            let new_env = env.insert(name, value.clone());
                            let new_remaining: Vec<_> = remaining[1..].to_vec();
                            
                            if new_remaining.is_empty() {
                                // All bindings done, evaluate body with all bindings in scope
                                state = State::Eval(body, new_env);
                            } else {
                                // Get the next binding's expression before moving new_remaining
                                let next_expr = new_remaining[0].1.clone();
                                // Push new Block frame and evaluate next binding's expression
                                stack.push(Frame::Block {
                                    bindings: bindings.clone(),
                                    remaining: new_remaining,
                                    body: body.clone(),
                                    env: new_env.clone(),
                                });
                                state = State::Eval(next_expr, new_env);
                            }
                        }
                        Frame::UnaryMinus => {
                            match value {
                                Value::Integer(n) => {
                                    let result = n.checked_neg()
                                        .ok_or_else(|| "runtime error: integer overflow".to_string())?;
                                    state = State::Return(Value::Integer(result));
                                }
                                Value::Rational(num, den) => {
                                    state = State::Return(Value::Rational(-*num, *den));
                                }
                                _ => return Err("runtime error: unary minus applied to non-number".to_string()),
                            }
                        }
                        Frame::Not => {
                            match value {
                                Value::Bool(b) => state = State::Return(Value::Bool(!b)),
                                _ => return Err("runtime error: ! applied to non-boolean".to_string()),
                            }
                        }
                        Frame::HandleScope { clauses: _ } => {
                            state = State::Return(value.clone());
                        }
                        Frame::PerformArgs { op, args, done, env } => {
                            let mut new_done = done.clone();
                            new_done.push(value.clone());
                            if new_done.len() == args.len() {
                                // All args evaluated, find handler
                                // Find the nearest HandleScope that has a clause for this op
                                let scope_result = stack.iter().rposition(|f| {
                                    if let Frame::HandleScope { clauses } = f {
                                        clauses.iter().any(|c| c.op_name == op)
                                    } else {
                                        false
                                    }
                                });
                                match scope_result {
                                    Some(idx) => {
                                        // Get the HandleScope data
                                        let clauses = if let Frame::HandleScope { clauses } = &stack[idx] {
                                            clauses.clone()
                                        } else {
                                            unreachable!();
                                        };
                                        
                                        // Find the clause
                                        let clause = match clauses.iter().find(|c| c.op_name == op) {
                                            Some(c) => c,
                                            None => unreachable!(),
                                        };
                                        
                                        // Capture: frames ABOVE the HandleScope become [f_bottom, ..., f_top]
                                        let above: Vec<Frame> = stack[idx+1..].to_vec();
                                        
                                        // Build continuation as [HandleScope, f_bottom, ..., f_top]
                                        let mut captured: Vec<Frame> = vec![];
                                        captured.push(Frame::HandleScope { clauses: clauses.clone() });
                                        captured.extend(above);
                                        
                                        let k = Value::Continuation(Continuation { frames: captured });
                                        
                                        // Build handler env: empty env + params + k
                                        let mut handler_env = Env::new();
                                        for (param, val) in clause.op_params.iter().zip(new_done.iter()) {
                                            handler_env.bindings.insert(param.clone(), val.clone());
                                        }
                                        handler_env.bindings.insert("k".to_string(), k);
                                        
                                        // Remove everything from idx onwards (including HandleScope)
                                        // The PerformArgs frame is already popped by the match
                                        stack.truncate(idx);
                                        
                                        state = State::Eval(clause.body.clone(), Rc::new(handler_env));
                                    }
                                    None => {
                                        return Err(format!("runtime error: effect '{}' performed but not handled", op));
                                    }
                                }
                            } else {
                                let next_idx = new_done.len();
                                stack.push(Frame::PerformArgs {
                                    op: op.clone(),
                                    args: args.clone(),
                                    done: new_done,
                                    env: env.clone(),
                                });
                                state = State::Eval(args[next_idx].clone(), env);
                            }
                        }
                        Frame::KCallArg { cont } => {
                            // k(value) - resume continuation
                            // Push captured frames in stored order and return value
                            for frame in cont.frames.iter() {
                                stack.push(frame.clone());
                            }
                            state = State::Return(value.clone());
                        }
                    }
                } else {
                    return Ok(value.clone());
                }
            }
        }
    }
}

fn apply_binop(op: &BinOp, left: Value, right: Value) -> Result<Value, String> {
    match op {
        BinOp::Add => left.add(right),
        BinOp::Sub => left.sub(right),
        BinOp::Mul => left.mul(right),
        BinOp::Div => left.div(right),
        BinOp::Less => compare_values(&left, &right, |a, b| a < b),
        BinOp::LessEqual => compare_values(&left, &right, |a, b| a <= b),
        BinOp::Greater => compare_values(&left, &right, |a, b| a > b),
        BinOp::GreaterEqual => compare_values(&left, &right, |a, b| a >= b),
        BinOp::Equal => equals_values(&left, &right),
        BinOp::NotEqual => {
            let eq = equals_values(&left, &right)?;
            match eq {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err("internal error: equals_values returned non-bool".to_string()),
            }
        }
        BinOp::And | BinOp::Or => {
            Err("internal error: and/or should use short-circuit frames".to_string())
        }
    }
}

fn compare_values<F>(left: &Value, right: &Value, cmp: F) -> Result<Value, String>
where
    F: FnOnce(f64, f64) -> bool,
{
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(cmp(*a as f64, *b as f64))),
        (Value::Integer(a), Value::Rational(bn, bd)) => {
            Ok(Value::Bool(cmp(*a as f64, *bn as f64 / *bd as f64)))
        }
        (Value::Rational(an, ad), Value::Integer(b)) => {
            Ok(Value::Bool(cmp(*an as f64 / *ad as f64, *b as f64)))
        }
        (Value::Rational(an, ad), Value::Rational(bn, bd)) => {
            Ok(Value::Bool(cmp(*an as f64 / *ad as f64, *bn as f64 / *bd as f64)))
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
