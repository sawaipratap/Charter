# Project history

A log of how Charter got to its current state, and what's next. Written as
we go, so future-you (or anyone else) can see the reasoning, not just the
end result.

## Where it started

The idea began as "I want to build something like jacquard-lang" — a
reference language whose whole premise is that AI models increasingly write
code and humans review it, so the language should make a function's risk
*visible* (what side effects it might perform) instead of making a reviewer
read every line to find out.

## Figuring out what we actually wanted

Before writing anything, we worked out which parts of that reference were
worth carrying over and why, in plain language, one idea at a time:

1. **Effects are declared** — a function's signature states what external
   side effects it may perform (network, files, clock, randomness, ...),
   and the compiler enforces that it doesn't do anything undeclared.
2. **Effects are handled, not hardwired** — a function doesn't call
   something risky directly; it "performs" an operation and something
   else, a *handler*, decides how to respond. This is what makes safe
   testing possible (swap in a fake handler) without touching the
   function's code.
3. **Handlers that can resume more than once** — the same mechanism as
   #2, but a handler is allowed to continue the paused computation
   multiple times instead of once. Worked through with two examples: a
   coin-flip "clone yourself at each branch" story, and a full numeric
   walkthrough of a medical test (prior 1% illness rate, 90%/5% true/false
   positive rates) showing how `sample` (branch on every possibility) and
   `observe` (prune branches that contradict known facts, renormalize the
   rest) combine to give exact probability answers — not estimates — as
   ordinary code on top of the handler mechanism, not a bolted-on
   statistics engine.
4. **Content-addressing** — definitions are identified by a hash of their
   structure, not their name, so renaming/reformatting never silently
   breaks something that depends on them.

We also looked at a second reference project, `bendlang/bend`, which
tackles the same underlying problem ("how do you trust AI-written code")
with a completely different answer: mathematical proof-checking
(`LAWS.bend`: state an invariant, the compiler demands a proof it holds)
plus automatic parallelism, instead of jacquard's "make risk visible to a
human" approach. Conclusion: interesting contrast, but proofs and
parallelism are orthogonal to what we're building and not something to
fold in — the one actually useful data point from Bend was that it *also*
identifies packages by hash, independently landing on the same
content-addressing idea.

## Settling the goals and the plan

Asked directly why any of this mattered, which surfaced the real
objective: **recreate jacquard's "AI writes, humans review" niche, and
along the way, genuinely learn how a programming language is built** —
not just copy a feature list.

Decisions made:
- **Implementation language:** Rust.
- **Scope:** a real interpreter first, then an ahead-of-time native
  compiler later, once the interpreter defines correct behavior to check
  the compiler against.
- **Working style:** Claude writes each step and narrates what/why; the
  user jumps in with questions whenever something's unclear, rather than
  writing the code hands-on.

Build order agreed on (each step adds exactly one new idea on top of a
working system from the step before):

| Step | Adds |
|---|---|
| 0 | Lexer → parser → evaluator for plain arithmetic (no variables/functions yet) |
| 1 | Variables, functions, recursion, `if` — ordinary language features |
| 2 | Effect declarations, checked before running, but not yet handle-able |
| 3 | Real effect handlers, single-answer (perform / handle) |
| 4 | Multi-answer handlers, `sample` / `observe`, exact probabilistic inference |
| 5 | Content-addressed definitions, hash-based store |
| 6 | Ahead-of-time native compiler |

Rationale for the order: steps 0–1 are "boring" on purpose, to get the
basic machinery (trees in memory, environments, closures) working before
anything exotic. Step 3 builds the hard part (representing "the rest of
the program" as something you can hand around and call) at its simplest,
single-answer form; step 4 reuses that same machinery and just relaxes a
rule (let it resume more than once) to unlock probability. Step 5 is
mostly independent of 2–4. Step 6 is deliberately last — compiling a
language is much easier once a working interpreter already defines exactly
what every program is supposed to do.

## What's been built so far

Nothing beyond design and documentation yet — no lexer, parser, or
interpreter code exists in the repository. What exists:

- **`README.md`** (committed, pushed to `claude/vigilant-johnson-g43mg1`) —
  describes Charter's motivation, the four core ideas above, a worked
  example connecting them to the "AI writes, human reviews" goal, the
  stage 0–6 architecture table, and current status. Written to stand on
  its own as an original design description.
- **A project-details spreadsheet** (Project Title / Abstract / Name / SAP
  ID), generated for an academic submission, delivered directly to the
  user rather than committed to the repo.
- One housekeeping fix: the README commit's authorship was changed from
  Claude to the user (Sawai Pratap Khatri), with the AI co-author line
  removed, at the user's request.

### Stage 0: Lexer, Parser, Evaluator for Arithmetic

Built and tested (59/59 tests pass). What was added:

- **`src/lexer.rs`** — turns raw text into tokens: numbers, operators
  (`+`, `-`, `*`, `/`), and parentheses. Whitespace is ignored.
- **`src/parse.rs`** — builds an abstract syntax tree (AST) from tokens.
  Uses a two-level recursive descent to respect operator precedence
  (`*` and `/` bind tighter than `+` and `-`), and handles unary minus
  (which can stack: `--5` equals `5`).
- **`src/eval.rs`** — walks the AST and produces a value. Division is
  exact rational arithmetic: `7/2` prints `7/2`, `6/4` simplifies to
  `3/2`, and `6/3` prints `2`. Negative numbers are stored as rationals
  with a negative numerator, so `--5` correctly reduces to `5/1`.
- **`src/lib.rs`** — defines the `Value` enum (`Integer` or `Rational`),
  the `Expr` AST type, and re-exports `lex`, `parse`, `eval`.
- **`src/main.rs`** — CLI entry point. `charter run <file>` reads the
  file, runs it through the pipeline, and prints the result. Errors go
  to stderr with a prefix (`lex error:`, `parse error:`, `runtime error:`)
  and exit non-zero.

Design decisions:
- **Exact rationals**: chosen so Stage 4's probabilistic inference stays
  exact, not approximate. Uses `i64` for numerator and denominator;
  overflow is a runtime error (or lex error for literals > i64::MAX).
- **Two-level precedence**: `parse_expr` handles `+`/`-`, `parse_term`
  handles `*`/`/`. Clean and extendable if we add more precedence levels.
- **Rational reduction**: GCD to lowest terms, denominator always positive.

## What's next

### Stage 1: Variables, Functions, Recursion, if/else

Built and tested (84/84 tests pass). What was added:

- **`src/lexer.rs`** — Extended with keywords (`fn`, `let`, `if`, `else`,
  `true`, `false`), identifiers, multi-character operators (`&&`, `||`,
  `==`, `!=`, `<=`, `>=`), and braces/semicolons for blocks.
- **`src/parse.rs`** — Complete rewrite with function definitions, let
  bindings, blocks, if/else expressions, and proper precedence climbing.
  Static checks for function references, arity, chained comparisons.
- **`src/eval.rs`** — Environment-based evaluation with lexical scope,
  function closures, recursion support, short-circuit && and ||.
- **`src/lib.rs`** — Extended `Value` with `Bool` variant, type-safe
  arithmetic with error messages.

Design decisions:
- **Lexical scope**: variables are bound in their defining block; inner
  blocks can shadow outer bindings.
- **Functions as closures**: capture the environment at definition time.
- **Static checks**: function existence, arity, chained comparisons detected
  at parse/check time; variable bindings checked statically with scope.
- **Recursion**: plain call depth tracked by incrementing a counter on
  each function call; limit is 10,000. Programs that exceed it get a clean
  `runtime error: recursion limit exceeded` instead of crashing.

### Stage 2: Effect declarations and checking

Built and tested (55/55 tests pass). What was added:

- **Effect declarations**: `effect Name { op1(args), op2(), ... }` at any
  position in the program.
- **Uses clause**: `fn f() uses [Effect1, Effect2] { ... }` declares
  which effects a function may perform.
- **Perform**: `perform op(args)` expression marks effect usage.
- **Effect checking**: All effects used (direct performs + transitive
  through function calls) must be declared in the function's uses.
  Top-level expression cannot use effects (nothing handles them yet).
- **Arity checking**: Both function calls and perform operations check
  argument count at static-check time.

Design decisions:
- **Effect ops are global**: operation names must be unique across all
  effects.
- **Uses is a subset check**: a function's used effects (direct performs
  + transitive) must be a subset of its declared uses.
- **Declarations can be interleaved**: effects, functions, and the main
  expression can appear in any order.

### Stage 3: Effect handlers

Stack machine with defunctionalized continuations (commit 4b26419):
- enum Frame: BinLeft, BinRight, Let, CallArgs, CallBody, If, Block, UnaryMinus, Not, HandleScope{clauses, env}, PerformArgs, KCallArg
- Continuation: frames above HandleScope as [HandleScope, f_bottom, ..., f_top]
- k(value): push frames in stored order, State::Return(value)
- Short-circuit && and ||
- HandleScope lookup finds scope with matching op clause
- Handler env starts from handle expression's env, then params, then k

Static checks:
- Handler clause arity must match operation arity
- Duplicate handler clauses rejected
- Unknown operations in handler rejected
- k arity must be 1 argument
- Clause body effects checked against enclosing context

Test results: 56/56 passing (commit 4b26419)
