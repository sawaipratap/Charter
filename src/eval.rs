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

/// Represents a rational number
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rational(i64, i64);  // numerator, denominator (always positive denominator)

impl Rational {
    fn new(n: i64, d: i64) -> Self {
        if d < 0 {
            Rational(-n, -d)
        } else {
            Rational(n, d)
        }
    }
    
    fn zero() -> Self { Rational(0, 1) }
    
    fn one() -> Self { Rational(1, 1) }
    
    fn mul(&self, other: &Rational) -> Self {
        Rational(self.0 * other.0, self.1 * other.1).normalize()
    }
    
    fn add(&self, other: &Rational) -> Self {
        // a/b + c/d = (ad + bc) / bd
        Rational(self.0 * other.1 + other.0 * self.1, self.1 * other.1).normalize()
    }
    
    fn normalize(&self) -> Self {
        if self.0 == 0 {
            return Rational(0, 1);
        }
        let g = gcd(self.0.abs(), self.1);
        Rational(self.0 / g, self.1 / g)
    }
    
    fn to_value(&self) -> Value {
        if self.1 == 1 {
            Value::Integer(self.0)
        } else {
            Value::Rational(self.0, self.1)
        }
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

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
    Distribution(Vec<(Value, (i64, i64))>),  // value -> (numerator, denominator)
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
    HandleScope { clauses: Vec<HandlerClause>, env: Rc<Env> },
    PerformArgs { op: String, args: Vec<Expr>, done: Vec<Value>, env: Rc<Env> },
    KCallArg { cont: Continuation },
    
    // Frames for infer (Stage 4)
    /// Branches from flip/choose with remaining branches to explore
    InferBranch { remaining: Vec<(Value, Rational)>, current_weight: Rational, result: Option<Value> },
    /// Waiting for continuation to be bound (for flip)
    InferFlip { p: Value, weight: Rational },
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
                                    if let Frame::HandleScope { clauses, env: _ } = f {
                                        clauses.iter().any(|c| c.op_name == op)
                                    } else {
                                        false
                                    }
                                });
                                match scope_result {
                                    Some(idx) => {
                                        let (clauses, scope_env) = if let Frame::HandleScope { clauses, env } = &stack[idx] {
                                            (clauses.clone(), env.clone())
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
                                        captured.push(Frame::HandleScope { clauses: clauses.clone(), env: scope_env.clone() });
                                        captured.extend(above);
                                        
                                        let k = Value::Continuation(Continuation { frames: captured });
                                        
                                        // Build handler env: scope env + params + k
                                        let mut handler_env = (*scope_env).clone();
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
                            env: env.clone(),
                        });
                        state = State::Eval((**body).clone(), env.clone());
                    }
                    Expr::Infer(body) => {
                        // Run inference on the body
                        return eval_infer(body, env.clone(), functions);
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
                        Frame::HandleScope { clauses: _, env: _ } => {
                            state = State::Return(value.clone());
                        }
                        Frame::PerformArgs { op, args, done, env } => {
                            let mut new_done = done.clone();
                            new_done.push(value.clone());
                            if new_done.len() == args.len() {
                                // All args evaluated, find handler
                                // Find the nearest HandleScope that has a clause for this op
                                let scope_result = stack.iter().rposition(|f| {
                                    if let Frame::HandleScope { clauses, env: _ } = f {
                                        clauses.iter().any(|c| c.op_name == op)
                                    } else {
                                        false
                                    }
                                });
                                match scope_result {
                                    Some(idx) => {
                                        // Get the HandleScope data
                                        let (clauses, scope_env) = if let Frame::HandleScope { clauses, env } = &stack[idx] {
                                            (clauses.clone(), env.clone())
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
                                        captured.push(Frame::HandleScope { clauses: clauses.clone(), env: scope_env.clone() });
                                        captured.extend(above);
                                        
                                        let k = Value::Continuation(Continuation { frames: captured });
                                        
                                        // Build handler env: scope env + params + k
                                        let mut handler_env = (*scope_env).clone();
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
                        Frame::InferBranch { .. } => {
                            // Should not happen in normal evaluation
                            return Err("internal error: InferBranch in normal evaluation".to_string());
                        }
                        Frame::InferFlip { .. } => {
                            // Should not happen in normal evaluation
                            return Err("internal error: InferFlip in normal evaluation".to_string());
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
        (Value::Distribution(_), _) | (_, Value::Distribution(_)) => {
            Err("runtime error: cannot compare distributions".to_string())
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

/// Evaluate an infer expression with exact inference
fn eval_infer(body: &Expr, env: Rc<Env>, functions: &HashMap<String, Function>) -> Result<Value, String> {
    // Transform the body: wrap Prob operations in special function calls
    let transformed = transform_for_infer(body.clone())?;
    
    // Add infer helper functions to the function map
    let mut infer_fns = functions.clone();
    add_infer_functions(&mut infer_fns);
    
    // Run the transformed expression with inference semantics
    // The body now calls __infer_flip, __infer_choose, __infer_observe
    // which explore all branches
    let mut results: Vec<(Value, Rational)> = Vec::new();
    run_inference(transformed, env, &infer_fns, Rational::one(), &mut results)?;
    
    // Pool equal values and normalize
    pool_and_normalize(results)
}

/// Transform an expression - for infer, we just recurse into the body
fn transform_for_infer(expr: Expr) -> Result<Expr, String> {
    match expr {
        Expr::Perform(op_name, args) => {
            // Don't transform perform - we'll handle it specially in run_inference
            Ok(Expr::Perform(op_name, args))
        }
        Expr::Block(bindings, body) => {
            let mut new_bindings: Vec<(String, Expr)> = Vec::new();
            for (name, e) in bindings {
                new_bindings.push((name, transform_for_infer(e)?));
            }
            Ok(Expr::Block(new_bindings, Box::new(transform_for_infer(*body)?)))
        }
        Expr::If(cond, then, else_) => {
            Ok(Expr::If(
                Box::new(transform_for_infer(*cond)?),
                Box::new(transform_for_infer(*then)?),
                Box::new(transform_for_infer(*else_)?),
            ))
        }
        Expr::Binary(left, op, right) => {
            Ok(Expr::Binary(
                Box::new(transform_for_infer(*left)?),
                op,
                Box::new(transform_for_infer(*right)?),
            ))
        }
        Expr::UnaryMinus(inner) => {
            Ok(Expr::UnaryMinus(Box::new(transform_for_infer(*inner)?)))
        }
        Expr::Not(inner) => {
            Ok(Expr::Not(Box::new(transform_for_infer(*inner)?)))
        }
        Expr::Call(name, args) => {
            let new_args: Result<Vec<_>, _> = args.into_iter()
                .map(|a| transform_for_infer(a))
                .collect();
            Ok(Expr::Call(name, new_args?))
        }
        Expr::Handle(body, clauses) => {
            // Don't transform inside handle - it handles its own effects
            Ok(Expr::Handle(body, clauses))
        }
        Expr::Infer(_) => {
            Err("runtime error: nested infer not supported".to_string())
        }
        other => Ok(other),
    }
}

/// Add infer helper functions to the function map
fn add_infer_functions(_functions: &mut HashMap<String, Function>) {
    // The infer functions are handled specially by run_inference, not as regular functions
}

/// Run inference on an expression, exploring all branches
fn run_inference(
    expr: Expr,
    env: Rc<Env>,
    functions: &HashMap<String, Function>,
    weight: Rational,
    results: &mut Vec<(Value, Rational)>,
) -> Result<(), String> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut state = State::Eval(expr, env);
    let mut current_weight = weight;
    
    loop {
        match state {
            State::Eval(e, env) => {
                match e {
                    Expr::Number(n) => state = State::Return(Value::Integer(n)),
                    Expr::Boolean(b) => state = State::Return(Value::Bool(b)),
                    Expr::Ident(name) => {
                        if name == "k" {
                            return Err("runtime error: continuation 'k' used as value".to_string());
                        }
                        match env.get(&name) {
                            Some(v) => state = State::Return(v),
                            None => return Err(format!("runtime error: unbound variable '{}'", name)),
                        }
                    }
                    Expr::Call(name, args) => {
                        // Check for special infer functions
                        if name == "__infer_flip" {
                            // flip(p, k): call k(true) with prob p, k(false) with prob 1-p
                            let p = eval_simple(args[0].clone(), env.clone())?;
                            let k_val = eval_simple(args[1].clone(), env.clone())?;
                            if let Value::Continuation(cont) = k_val {
                                // Get probability
                                let (prob_num, prob_den) = match p {
                                    Value::Integer(n) => (n, 1),
                                    Value::Rational(n, d) => (n, d),
                                    _ => return Err("runtime error: flip requires a number".to_string()),
                                };
                                let prob = Rational(prob_num, prob_den);
                                let true_weight = current_weight.mul(&prob);
                                let one_minus = Rational(1, 1).add(&Rational(-prob_num, prob_den));
                                let false_weight = current_weight.mul(&one_minus);
                                
                                // Explore true branch
                                let mut true_results: Vec<(Value, Rational)> = Vec::new();
                                resume_infer(cont.clone(), Value::Bool(true), &mut true_results)?;
                                for (v, w) in true_results {
                                    results.push((v, true_weight.mul(&w)));
                                }
                                
                                // Explore false branch
                                let mut false_results: Vec<(Value, Rational)> = Vec::new();
                                resume_infer(cont, Value::Bool(false), &mut false_results)?;
                                for (v, w) in false_results {
                                    results.push((v, false_weight.mul(&w)));
                                }
                            }
                            return Ok(());
                        } else if name == "__infer_choose" {
                            // choose(k, a, b, ...): call k(a), k(b), ... each with equal prob
                            let k_val = eval_simple(args[0].clone(), env.clone())?;
                            let choices: Vec<Value> = args.iter().skip(1)
                                .map(|a| eval_simple(a.clone(), env.clone()))
                                .collect::<Result<Vec<_>, _>>()?;
                            if let Value::Continuation(cont) = k_val {
                                let n = choices.len() as i64;
                                let branch_weight = Rational(1, n);
                                for choice in choices {
                                    let mut choice_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont.clone(), choice, &mut choice_results)?;
                                    for (v, w) in choice_results {
                                        results.push((v, branch_weight.mul(&w)));
                                    }
                                }
                            }
                            return Ok(());
                        } else if name == "__infer_observe" {
                            // observe(cond, k): if cond is true, call k(); else prune
                            let cond = eval_simple(args[0].clone(), env.clone())?;
                            let k_val = eval_simple(args[1].clone(), env.clone())?;
                            if let Value::Bool(true) = cond {
                                if let Value::Continuation(cont) = k_val {
                                    let mut k_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont, Value::Bool(true), &mut k_results)?;
                                    for (v, w) in k_results {
                                        results.push((v, current_weight.mul(&w)));
                                    }
                                }
                            }
                            // If false, prune (don't add any results)
                            return Ok(());
                        } else if let Some(func) = functions.get(&name) {
                            // Regular function call
                            if args.len() != func.params.len() {
                                return Err(format!("runtime error: function '{}' expected {} arguments, got {}",
                                    name, func.params.len(), args.len()));
                            }
                            
                            if args.is_empty() {
                                let mut new_env = Env::new();
                                state = State::Eval(func.body.clone(), Rc::new(new_env));
                            } else {
                                let first_arg = args[0].clone();
                                let args_clone = args.clone();
                                let env_clone = env.clone();
                                let env_for_eval = env.clone();
                                stack.push(Frame::CallArgs {
                                    name: name.clone(),
                                    params: func.params.clone(),
                                    args: args_clone,
                                    done: Vec::new(),
                                    env: env_clone,
                                });
                                state = State::Eval(first_arg, env_for_eval);
                            }
                        } else {
                            return Err(format!("runtime error: undefined function '{}'", name));
                        }
                    }
                    Expr::Binary(left, op, right) => {
                        stack.push(Frame::BinLeft { op, right: *right, env: env.clone() });
                        state = State::Eval(*left, env);
                    }
                    Expr::UnaryMinus(inner) => {
                        stack.push(Frame::UnaryMinus);
                        state = State::Eval(*inner, env);
                    }
                    Expr::Not(inner) => {
                        stack.push(Frame::Not);
                        state = State::Eval(*inner, env);
                    }
                    Expr::If(cond, then, else_) => {
                        stack.push(Frame::If { then_branch: *then, else_branch: *else_, env: env.clone() });
                        state = State::Eval(*cond, env);
                    }
                    Expr::Block(bindings, body) => {
                        if bindings.is_empty() {
                            state = State::Eval(*body, env);
                        } else {
                            let bindings_clone = bindings.clone();
                            let (name, expr) = bindings_clone[0].clone();
                            let remaining: Vec<_> = bindings_clone.into_iter().skip(1).collect();
                            stack.push(Frame::Block {
                                bindings: remaining.clone(),
                                remaining: remaining,
                                body: *body,
                                env: env.clone(),
                            });
                            state = State::Eval(expr, env);
                        }
                    }
                    Expr::Perform(op_name, args) => {
                        // Handle Prob operations specially
                        match op_name.as_str() {
                            "flip" => {
                                // flip(p): explore k(true) and k(false)
                                let p = eval_simple(args[0].clone(), env.clone())?;
                                
                                // Get probability
                                let (prob_num, prob_den) = match p {
                                    Value::Integer(n) => (n, 1),
                                    Value::Rational(n, d) => (n, d),
                                    _ => return Err("runtime error: flip requires a number".to_string()),
                                };
                                let prob = Rational(prob_num, prob_den);
                                let true_weight = current_weight.mul(&prob);
                                let one_minus = Rational(1, 1).add(&Rational(-prob_num, prob_den));
                                let false_weight = current_weight.mul(&one_minus);
                                
                                // The continuation is on the stack - capture it
                                // We need to create a continuation from the current stack
                                let cont = Continuation { frames: stack.clone() };
                                
                                // Explore true branch only if probability > 0
                                if true_weight.0 != 0 {
                                    let mut true_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont.clone(), Value::Bool(true), &mut true_results)?;
                                    for (v, w) in true_results {
                                        results.push((v, true_weight.mul(&w)));
                                    }
                                }
                                
                                // Explore false branch only if probability > 0
                                if false_weight.0 != 0 {
                                    let mut false_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont, Value::Bool(false), &mut false_results)?;
                                    for (v, w) in false_results {
                                        results.push((v, false_weight.mul(&w)));
                                    }
                                }
                                return Ok(());
                            }
                            "choose" => {
                                // choose(a, b, ...): explore k(a), k(b), ... each with equal prob
                                let choices: Vec<Value> = args.iter()
                                    .map(|a| eval_simple(a.clone(), env.clone()))
                                    .collect::<Result<Vec<_>, _>>()?;
                                
                                let n = choices.len() as i64;
                                let branch_weight = Rational(1, n);
                                
                                // Capture continuation
                                let cont = Continuation { frames: stack.clone() };
                                
                                for choice in choices {
                                    let mut choice_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont.clone(), choice, &mut choice_results)?;
                                    for (v, w) in choice_results {
                                        results.push((v, branch_weight.mul(&w)));
                                    }
                                }
                                return Ok(());
                            }
                            "observe" => {
                                // observe(cond): if cond is true, continue; else prune
                                let cond = eval_simple(args[0].clone(), env.clone())?;
                                if let Value::Bool(true) = cond {
                                    // Continue - the stack is the continuation
                                    let cont = Continuation { frames: stack.clone() };
                                    let mut k_results: Vec<(Value, Rational)> = Vec::new();
                                    resume_infer(cont, Value::Bool(true), &mut k_results)?;
                                    for (v, w) in k_results {
                                        results.push((v, current_weight.mul(&w)));
                                    }
                                }
                                // If false, prune (don't add any results)
                                return Ok(());
                            }
                            _ => {
                                return Err(format!("runtime error: effect '{}' not handled in infer", op_name));
                            }
                        }
                    }
                    Expr::Handle(body, clauses) => {
                        stack.push(Frame::HandleScope { clauses, env: env.clone() });
                        state = State::Eval(*body, env);
                    }
                    Expr::Infer(_) => {
                        return Err("runtime error: nested infer not supported".to_string());
                    }
                }
            }
            State::Return(value) => {
                match stack.pop() {
                    Some(Frame::BinLeft { op, right, env }) => {
                        match right {
                            Expr::Number(_) | Expr::Boolean(_) | Expr::Ident(_) => {
                                let right_val = eval_simple(right, env.clone())?;
                                let result = apply_binop(&op, value, right_val)?;
                                state = State::Return(result);
                            }
                            _ => {
                                stack.push(Frame::BinRight { op, left_val: value });
                                state = State::Eval(right, env);
                            }
                        }
                    }
                    Some(Frame::BinRight { op, left_val }) => {
                        let result = apply_binop(&op, left_val, value)?;
                        state = State::Return(result);
                    }
                    Some(Frame::UnaryMinus) => {
                        match value {
                            Value::Integer(n) => state = State::Return(Value::Integer(-n)),
                            Value::Rational(n, d) => state = State::Return(Value::Rational(-n, d)),
                            _ => return Err("runtime error: unary minus applied to non-number".to_string()),
                        }
                    }
                    Some(Frame::Not) => {
                        match value {
                            Value::Bool(b) => state = State::Return(Value::Bool(!b)),
                            _ => return Err("runtime error: ! applied to non-boolean".to_string()),
                        }
                    }
                    Some(Frame::If { then_branch, else_branch, env }) => {
                        match value {
                            Value::Bool(true) => state = State::Eval(then_branch, env),
                            Value::Bool(false) => state = State::Eval(else_branch, env),
                            _ => return Err("runtime error: if condition must be boolean".to_string()),
                        }
                    }
                    Some(Frame::Block { bindings, body, env, remaining: _ }) => {
                        state = State::Eval(Expr::Block(bindings, Box::new(body)), env);
                    }
                    Some(Frame::CallArgs { name, params, args, done, env }) => {
                        let mut new_done = done;
                        new_done.push(value);
                        if new_done.len() == args.len() {
                            if let Some(func) = functions.get(&name) {
                                let mut new_env = Env::new();
                                for (param, val) in params.iter().zip(new_done.iter()) {
                                    new_env.bindings.insert(param.clone(), val.clone());
                                }
                                state = State::Eval(func.body.clone(), Rc::new(new_env));
                            } else {
                                return Err(format!("runtime error: undefined function '{}'", name));
                            }
                        } else {
                            let next_idx = new_done.len();
                            let args_clone = args.clone();
                            let env_clone = env.clone();
                            stack.push(Frame::CallArgs { name, params, args, done: new_done, env });
                            state = State::Eval(args_clone[next_idx].clone(), env_clone);
                        }
                    }
                    Some(Frame::HandleScope { clauses: _, env: _ }) => {
                        state = State::Return(value);
                    }
                    Some(Frame::PerformArgs { .. }) => {
                        return Err("internal error: PerformArgs in inference".to_string());
                    }
                    Some(Frame::KCallArg { cont }) => {
                        // Multi-shot: k(value) - resume continuation
                        let mut cont_results: Vec<(Value, Rational)> = Vec::new();
                        resume_with_stack(cont, value, &mut cont_results)?;
                        for (v, w) in cont_results {
                            results.push((v, current_weight.mul(&w)));
                        }
                        return Ok(());
                    }
                    Some(Frame::InferBranch { .. }) | Some(Frame::InferFlip { .. }) => {
                        return Err("internal error: unexpected frame in inference".to_string());
                    }
                    Some(Frame::Let { name, body, env }) => {
                        let new_env = env.insert(name, value);
                        state = State::Eval(body.clone(), new_env);
                    }
                    Some(Frame::CallBody { name, params, args_vals, closure_env: _ }) => {
                        if let Some(func) = functions.get(&name) {
                            let mut new_env = Env::new();
                            for (param, val) in params.iter().zip(args_vals.iter()) {
                                new_env.bindings.insert(param.clone(), val.clone());
                            }
                            state = State::Eval(func.body.clone(), Rc::new(new_env));
                        } else {
                            return Err(format!("runtime error: undefined function '{}'", name));
                        }
                    }
                    None => {
                        // Final result
                        results.push((value, current_weight));
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Resume a continuation with a value
fn resume_infer(
    cont: Continuation,
    value: Value,
    results: &mut Vec<(Value, Rational)>,
) -> Result<(), String> {
    let mut cont_results: Vec<(Value, Rational)> = Vec::new();
    resume_with_stack(cont, value, &mut cont_results)?;
    results.extend(cont_results);
    Ok(())
}

/// Resume with a continuation's stack
fn resume_with_stack(
    cont: Continuation,
    value: Value,
    results: &mut Vec<(Value, Rational)>,
) -> Result<(), String> {
    // The continuation captures: [HandleScope, frames...]
    // We need to find the HandleScope and extract k
    for (i, frame) in cont.frames.iter().enumerate() {
        if let Frame::HandleScope { clauses, env } = frame {
            // Find the first clause (for now, assume single clause handles all)
            // In infer, the clauses handle flip/choose/observe
            // We need to provide k to the continuation
            
            // The body after perform should be in the remaining frames
            // For now, just push the value as result if no more frames
            if i == cont.frames.len() - 1 {
                results.push((value, Rational::one()));
                return Ok(());
            }
            
            // Continue with remaining frames
            let remaining: Vec<Frame> = cont.frames[i+1..].to_vec();
            let mut stack = remaining;
            let mut state = State::Return(value);
            
            loop {
                match state {
                    State::Return(val) => {
                        if let Some(f) = stack.pop() {
                            match f {
                                Frame::KCallArg { cont: c } => {
                                    // Multi-shot: continue with the inner continuation
                                    resume_with_stack(c, val, results)?;
                                    return Ok(());
                                }
                                Frame::HandleScope { .. } => {
                                    results.push((val, Rational::one()));
                                    return Ok(());
                                }
                                Frame::Block { bindings, body, env, remaining: _ } => {
                                    state = State::Eval(Expr::Block(bindings, Box::new(body)), env);
                                }
                                _ => {
                                    return Err("internal error: unexpected frame in continuation".to_string());
                                }
                            }
                        } else {
                            results.push((val, Rational::one()));
                            return Ok(());
                        }
                    }
                    State::Eval(e, env) => {
                        match e {
                            Expr::Number(n) => state = State::Return(Value::Integer(n)),
                            Expr::Boolean(b) => state = State::Return(Value::Bool(b)),
                            Expr::Ident(name) => {
                                match env.get(&name) {
                                    Some(v) => state = State::Return(v),
                                    None => return Err(format!("runtime error: unbound variable '{}'", name)),
                                }
                            }
                            _ => return Err("internal error: complex expr in continuation".to_string()),
                        }
                    }
                }
            }
        }
    }
    
    // No HandleScope found, just return the value
    results.push((value, Rational::one()));
    Ok(())
}

/// Evaluate a simple expression to a value (no control flow)
fn eval_simple(expr: Expr, env: Rc<Env>) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Integer(n)),
        Expr::Boolean(b) => Ok(Value::Bool(b)),
        Expr::Ident(name) => {
            match env.get(&name) {
                Some(v) => Ok(v),
                None => Err(format!("runtime error: unbound variable '{}'", name)),
            }
        }
        Expr::Binary(left, BinOp::Div, right) => {
            // Handle division for rationals like 1/2
            let left_val = eval_simple(*left, env.clone())?;
            let right_val = eval_simple(*right, env)?;
            match (left_val, right_val) {
                (Value::Integer(a), Value::Integer(b)) if b != 0 => {
                    // Return as rational
                    Ok(Value::Rational(a, b))
                }
                _ => Err("runtime error: invalid division".to_string()),
            }
        }
        _ => Err("internal error: complex expression in eval_simple".to_string()),
    }
}

/// Pool equal values and normalize the distribution
fn pool_and_normalize(results: Vec<(Value, Rational)>) -> Result<Value, String> {
    // Pool equal values
    let mut pooled: Vec<(Value, (i64, i64))> = Vec::new();
    for (value, weight) in results {
        if let Some(existing) = pooled.iter_mut().find(|(v, _)| values_equal(&v, &value)) {
            existing.1 = (existing.1.0 + weight.0, existing.1.1);
        } else {
            pooled.push((value, (weight.0, weight.1)));
        }
    }
    
    // Normalize
    let total_r = pooled.iter()
        .map(|(_, (n, d))| Rational(*n, *d))
        .fold(Rational::zero(), |acc, r| acc.add(&r));
    
    if total_r.0 == 0 {
        return Err("runtime error: no possibilities survive".to_string());
    }
    
    // Normalize each probability
    let mut normalized: Vec<(Value, (i64, i64))> = pooled.into_iter()
        .map(|(v, (n, d))| {
            let prob = Rational(n, d).mul(&Rational(total_r.1, total_r.0));
            (v, (prob.0, prob.1))
        })
        .collect();
    
    // Sort by value
    normalized.sort_by(|(a, _), (b, _)| compare_values_order(a, b));
    
    Ok(Value::Distribution(normalized))
}

/// Check if two values are equal
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Integer(a), Value::Rational(bn, bd)) => (*a as f64) == (*bn as f64 / *bd as f64),
        (Value::Rational(an, ad), Value::Integer(b)) => (*an as f64 / *ad as f64) == (*b as f64),
        (Value::Rational(an, ad), Value::Rational(bn, bd)) => an * bd == bn * ad,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        _ => false,
    }
}

/// Compare values for sorting: false < true < numbers numerically
fn compare_values_order(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Bool(false), Value::Bool(true)) => std::cmp::Ordering::Less,
        (Value::Bool(true), Value::Bool(false)) => std::cmp::Ordering::Greater,
        (Value::Bool(false), _) => std::cmp::Ordering::Less,
        (_, Value::Bool(false)) => std::cmp::Ordering::Greater,
        (Value::Bool(true), _) => std::cmp::Ordering::Less,
        (_, Value::Bool(true)) => std::cmp::Ordering::Greater,
        (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
        (Value::Integer(a), Value::Rational(bn, bd)) => {
            let af = *a as f64;
            let bf = *bn as f64 / *bd as f64;
            af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::Rational(an, ad), Value::Integer(b)) => {
            let af = *an as f64 / *ad as f64;
            let bf = *b as f64;
            af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::Rational(an, ad), Value::Rational(bn, bd)) => {
            let af = *an as f64 / *ad as f64;
            let bf = *bn as f64 / *bd as f64;
            af.partial_cmp(&bf).unwrap_or(std::cmp::Ordering::Equal)
        }
        _ => std::cmp::Ordering::Equal,
    }
}
