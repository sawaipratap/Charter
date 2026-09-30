# Charter

Charter is a small programming language for a world where a lot of code gets
written by AI models and reviewed by people. Its central idea: a function's
signature should tell a reviewer everything they need to know about *how
risky it is*, without them having to read the whole implementation.

## The problem

When an AI writes a function, a human reviewer usually has to read the full
body to answer the only question that actually matters: *can this thing hurt
me?* Does it touch the network? Delete a file? Depend on randomness in a way
that makes it non-reproducible? None of that is visible from the outside in
most languages — a call that looks like `process(data)` could be doing
absolutely anything inside.

Charter's answer is to make that information part of the type system itself,
and have the compiler refuse to run code that lies about it.

## Core ideas

### 1. Effects are declared, and enforced

Every function states, as part of its signature, which external effects it
may perform:

```
fn get_weather(city) uses [network] {
    let response = http_get("https://weather.example/" + city)
    response.temperature
}
```

`uses [network]` is a contract. If `get_weather` (or anything it calls, at
any depth) tries to touch the filesystem, the clock, or anything else it
didn't declare, the compiler rejects the program before it ever runs. A
reviewer can trust the label instead of auditing the body.

### 2. Effects are handled, not hardwired

A function that needs something risky doesn't call it directly — it
*performs* an operation and waits for something else, called a **handler**,
to decide how to respond. This indirection is what makes the effect system
actually useful day to day:

- **Testing**: swap in a fake handler that returns canned data instead of
  making a real network call, with zero changes to the function under test.
- **Auditability**: a handler installed at the top of a program is the one
  place all instances of a given effect flow through, making it a natural
  point to log, rate-limit, or sandbox real-world side effects.

### 3. Handlers that can resume more than once

Ordinarily, when a handler answers a request, the paused function resumes
exactly once and moves on. Charter also allows a handler to resume the same
paused computation **multiple times** — effectively cloning the rest of the
program's execution once per possible answer. This one extra capability,
applied generally, is what turns a plain effect system into something that
can express:

- **Backtracking search** — try an option, and if a later step fails,
  resume from the choice point with the next option, instead of unwinding by
  hand.
- **Exact probabilistic reasoning** — a `sample` operation asks "give me one
  of these possibilities," and a handler can explore *all* of them, carrying
  a probability weight through each branch. An `observe` operation prunes
  away branches that contradict something already known to be true, and
  renormalizes the survivors. The result is exact Bayesian inference,
  written as ordinary library code on top of the same handler mechanism used
  for effects and testing — not a bolted-on statistics engine.

### 4. Code is identified by what it is, not what it's called

Top-level definitions are hashed from their canonical structure (not their
source text — comments, formatting, and variable naming don't change the
hash), and that hash is a definition's real identity. Human-readable names
are a separate lookup layer on top. This means renaming or reformatting a
function never silently breaks something that depends on it, and two
definitions that are structurally identical are recognized as the same thing
even if no one wrote them that way on purpose.

## Example: why this combination matters

An AI-written function that has to interpret something ambiguous — noisy
sensor data, a messy user input, a choice between plausible bug fixes —
often buries that judgment call inside opaque thresholds and `if`
statements. With effects and probabilistic reasoning built in, that judgment
becomes inspectable math instead of a hidden guess: a reviewer can see
exactly which evidence was weighed, and how, rather than reverse-engineering
an AI's implicit reasoning from the shape of its code.

## Architecture

Charter is implemented in Rust, and built up in stages, each adding one
capability on top of a working system from the previous stage:

| Stage | Adds |
|---|---|
| 0 | Lexer → parser → evaluator for plain arithmetic |
| 1 | Variables, functions, recursion, control flow |
| 2 | Effect declarations, checked (not yet handle-able) |
| 3 | Effect handlers, single-answer (perform / handle) |
| 4 | Multi-answer handlers, `sample` / `observe`, exact inference |
| 5 | Content-addressed definitions and a hash-based store |
| 6 | Ahead-of-time native compiler |

The interpreter (stages 0–5) is the semantic reference: it defines exactly
what every program should do. The native compiler (stage 6) comes last, once
that reference exists to check it against.

## Status

Early design and implementation. Nothing is built yet beyond this
description — see the stage table above for what's next.

## Goals

- Recreate, from first principles, a language purpose-built for a workflow
  where AI writes code and humans review it.
- Along the way, build a real understanding of how a language's pieces fit
  together: lexing, parsing, type/effect checking, interpretation via
  continuation-passing (needed for multi-shot handlers), and ahead-of-time
  compilation.
