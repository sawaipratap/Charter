#!/bin/sh
# Runs every Stage 0 test against a built charter binary.
# Usage: tests/stage0/run.sh [path-to-charter]   (default: target/debug/charter)
BIN="${1:-target/debug/charter}"; DIR="$(dirname "$0")"; pass=0; fail=0
for f in "$DIR"/*.charter; do
  b="${f%.charter}"; n="$(basename "$b")"
  out="$("$BIN" run "$f" 2>"${TMPDIR:-/tmp}/charter_err")"; code=$?
  if [ -f "$b.out" ]; then
    if [ $code -eq 0 ] && [ "$out" = "$(cat "$b.out")" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $n: want '$(cat "$b.out")' got '$out' (exit $code)"; fi
  else
    kind="$(cat "$b.reject")"
    if [ $code -ne 0 ] && [ -z "$out" ] && head -n1 "${TMPDIR:-/tmp}/charter_err" | grep -q "^$kind error"; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL $n: want '$kind error' exit!=0, got exit $code stdout '$out' stderr '$(head -n1 "${TMPDIR:-/tmp}/charter_err")'"; fi
  fi
done
echo "stage0: $pass passed, $fail failed"; [ $fail -eq 0 ]
