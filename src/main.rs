//! Charter CLI: run a Charter program from a file.
//!
//! Usage: charter run <file>
//!
//! Reads the file, lexes it, parses it, evaluates it, and prints the result.
//! Errors are printed to stderr with a prefix like "lex error:", "parse error:",
//! or "runtime error:".

use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 || args[1] != "run" {
        eprintln!("Usage: charter run <file>");
        process::exit(1);
    }

    let filename = &args[2];
    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file '{}': {}", filename, e);
            process::exit(1);
        }
    };

    // Strip trailing newline if present (test files often end with \n)
    let source = source.trim_end();

    // Lex: text -> tokens
    let tokens = match charter::lex(source) {
        Ok(t) => t,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    // Parse: tokens -> AST
    let ast = match charter::parse(tokens) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    // Evaluate: AST -> value
    let value = match charter::eval(ast) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    println!("{}", value.display());
}
