# Charter surface syntax and semantics, as fixed by the tests

README.md gives only one syntax sample, so the Test Reviewer fixed the rest here. Tests are the authority; if the engineer needs a change, ask the Test Reviewer, who will change the tests first.

Test format, CLI and rejection kinds: see stage0/README.md. Kinds are `lex`, `parse`, `check` (static: unbound names, arity, duplicates, unknown effects), `effect` (static: an effect is not declared or not handled; Stage 2 on) and `runtime`. A static error means nothing runs, so stdout stays empty.

## Stage 1: variables, functions, recursion, if
- Program = `fn` (and later `effect`) definitions plus exactly one top-level expression whose value is printed. Definitions may come before or after that expression (the implementation accepts both; tests only rely on before). Top-level functions can call each other and themselves in any order. No nested `fn`.
- `fn name(a, b) { block }`. Params may be empty. Duplicate function or parameter names are `check` errors.
- Block `{ let x = e; let y = e; final_expr }`: each `let` ends with `;`, a block ends with an expression, and a block is itself an expression. `let` may shadow; its right side cannot see the name it defines. Scope is lexical: a function sees only its parameters and its own lets, never the caller's.
- Values: exact rationals (Stage 0) and booleans `true`/`false`. Prints `true`/`false`.
- Operators, tightest first: unary `-` and `!`; `* /`; `+ -`; `< <= > >= == !=` (not chainable: `1 < 2 < 3` is a `parse` error); `&&`; `||`. `&&` and `||` short-circuit.
- `if cond { block } else { block }` is an expression; `else if` is allowed; `else` is required. Only the chosen branch runs.
- Calls `f(x, y)` evaluate arguments left to right. A call target must be a declared function (`check` error otherwise); wrong argument count is a `check` error.
- Type misuse is a `runtime` error: arithmetic on bools, `!` or `&&`/`||`/`if` on non-bools, `<` on bools, `==` between a number and a bool.
- Comments: `//` to end of line.
- Keywords (cannot be names): fn let if else true false uses effect perform handle with infer.
- Recursion is tested to depth 1000.

## Stage 2: effect declarations, checked before running
- `effect Name { op1(a, b), op2(), ... }` at top level (before or after functions; at least one operation, commas between, no trailing comma). Effect names start with a capital letter by convention only. Operation names are global: unique across all effects.
- `fn f(x) uses [Ask, Log] { ... }`: the effects the function may perform, directly or through anything it calls. `uses []` and omitting `uses` both mean pure. Listing more than needed is fine; listing a name twice or an undeclared effect is a `check` error.
- `perform op(args)`: an expression; its operand is exactly the call (`perform ask(1) + 2` is `(perform ask(1)) + 2`). Wrong argument count or unknown operation is a `check` error.
- Rule: the effects of a function body (its `perform`s plus the `uses` of every function it calls, at any depth) must be a subset of its `uses`; otherwise an `effect` error. The final top-level expression may use no effects: nothing handles them yet.
- All checks happen before anything runs, so a rejected program prints nothing even if the bad code is never reached. Accepted programs in this stage never actually execute a `perform`.

## Stage 3: single-answer handlers
- `handle { block } with { op(a, b) k => expr, op2() k => expr }`: runs the block; when it performs `op(v1, v2)`, the matching clause runs with `a`, `b` bound to the values and `k` bound to the paused rest of the block. Clauses are comma separated (trailing comma allowed, at least one clause). The clause body is any expression (use a block for several statements).
- `k(v)` resumes the paused code with `v` as the answer of the `perform`, and evaluates to the final value of the handled block. A clause may call `k` zero times (abort: the clause value becomes the value of the whole `handle`) or once; code may use the value of `k(v)` further (`k(p) + 1000`). `k` is only visible inside its clause; `k` takes exactly one argument.
- The value of the `handle` expression is the value of the block if it never performs a handled op, otherwise whatever the clause produced.
- Handlers are deep: the same handler stays installed when the code resumes, so later performs of the same op in the block are handled too.
- A perform of an op the nearest handler does not handle passes to the next enclosing handler. The innermost handler for an op wins.
- A clause runs outside its own handler: a `perform` inside a clause goes to the enclosing handlers, not to itself.
- Effect checking: a `handle` removes the effects of the ops it has clauses for from what its block needs. What is left (including whatever the clauses themselves perform) must still be declared by the enclosing function, or be empty at top level. A clause op that does not exist, or has a different parameter count, is a `check` error; two clauses for one op is a `check` error. A handle only covers code in its own block: a function that performs without declaring it is still an `effect` error even if every call to it is inside a handle.

## Stage 4: multi-answer handlers, exact inference
- **Multi-shot**: `k` may now be called any number of times in a clause, each call re-running the rest of the block from the `perform` (so side-effect-free code gives each call an independent copy). `k(a) + k(b)` is allowed.
- **Built-in effect `Prob`** (always declared; list it in `uses [Prob]`; user code cannot redefine the effect `Prob` or the operations below, a `check` error):
  - `perform flip(p)`: true with probability `p`, false with `1 - p`. `p` must be a number in [0, 1], else `runtime`.
  - `perform choose(a, b, ...)`: one of the (one or more) arguments, uniformly; equal values pool their weight.
  - `perform observe(cond)`: keeps only the possibilities where `cond` is true. `cond` must be a bool, else `runtime`.
- `infer { block }`: runs the block over every possibility, discharging `Prob` (only `Prob`; any other effect must still be handled or declared). Each possibility carries an exact probability; `observe` removes the false ones; the survivors are renormalized to sum to 1. The value of `infer` is a distribution, printed one line per distinct result value as `value: probability`, sorted ascending by value (`false` before `true`, numbers numerically), probabilities as reduced rationals (`1` when certain). Equal values pool their probabilities. If no possibility survives, `runtime` error. A distribution cannot be used in arithmetic or comparisons (`runtime`).
- Medical-test key example: prior 1% illness, 90% true positive, 5% false positive, observed positive: P(ill) = 0.009 / (0.009 + 0.0495) = 2/13 exactly.
