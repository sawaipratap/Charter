# Stage 0 tests: arithmetic

Tests are written before the code. The interpreter must pass all of them; they later serve as the reference for the native compiler.

## Format
Each test is a pair of files with the same base name:
- `name.charter`: the program (a single expression; whitespace and newlines are free).
- `name.out`: exact expected stdout, one line (the value, then `\n`). Exit code 0.
- or `reject_name.reject`: the program must be refused. The file holds the stage that must refuse it: `lex`, `parse` or `runtime`. Expected: exit code non-zero, empty stdout, first stderr line starting `<kind> error` (e.g. `parse error: ...`).

## Command
`charter run <file>` prints the value of the program. `run.sh [binary]` runs every test and prints failures plus a summary.

## Semantics fixed by these tests
- Operators: `+ - * /`, unary `-`, parentheses. `* /` bind tighter than `+ -`; all binary operators are left-associative; unary `-` can stack (`--5` is 5).
- Numbers are non-negative integer literals (digits only). No decimals, no variables, no `**`.
- **Division is exact (rational numbers), ASSUMPTION pending god's sign-off**: `7 / 2` prints `7/2`, `6 / 4` prints `3/2`, a whole result prints as an integer (`6 / 3` is `2`). Chosen so Stage 4 can give the exact posterior. Division by zero is a `runtime` rejection.
- Integer overflow is deliberately untested; the engineer should use big or checked integers and report the choice.
- Comments do not exist yet, so no test uses them.
