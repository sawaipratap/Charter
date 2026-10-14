# Charter surface syntax and semantics, as fixed by the tests

README.md gives only one syntax sample, so the Test Reviewer fixed the rest here. Tests are the authority; if the engineer needs a change, ask the Test Reviewer, who will change the tests first.

Test format, CLI and rejection kinds: see stage0/README.md. Kinds are `lex`, `parse`, `check` (static: unbound names, arity, duplicates, unknown effects), `effect` (static: an effect is not declared or not handled; Stage 2 on) and `runtime`. A static error means nothing runs, so stdout stays empty.

## Stage 1: variables, functions, recursion, if
- Program = zero or more `fn` definitions, then exactly one final expression whose value is printed. Top-level functions can call each other and themselves in any order. No nested `fn`.
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
