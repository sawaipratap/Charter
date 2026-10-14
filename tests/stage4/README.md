# Stage 4 tests: multi-answer handlers and exact inference

Same format as stage0/README.md, except `.out` may have several lines (a distribution, one `value: probability` line each); rules in ../SYNTAX.md. Key tests: `medical_test_exact` (prior 1%, 90% true positive, 5% false positive, posterior of illness 2/13) and `backtracking_first_solution` / `backtracking_count_all` / `backtracking_by_inference`. Stages 0-3 must still pass.
