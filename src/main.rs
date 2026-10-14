//! Charter CLI: run a Charter program from a file.

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
    let source = source.trim_end().to_string();

    // Run the interpreter (stack machine is iterative, no recursion issues)
    let result = charter::eval_string(&source);

    match result {
        Ok(value) => println!("{}", value.display()),
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    }
}
