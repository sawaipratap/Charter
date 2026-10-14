# Stage 2 tests: effect declarations

Same format as stage0/README.md; rules in ../SYNTAX.md. The `effect` rejection kind is new: a program that performs, or calls something that performs, an effect the enclosing function did not declare. Rejection is static, so even dead code is rejected. Stages 0 and 1 must still pass.
