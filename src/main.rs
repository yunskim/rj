mod array;
mod interp;
mod verbs;
mod eval;

use interp::Interpreter;
use eval::{tokenize, eval};
use std::io::{self, BufRead, Write};

fn main() {
    let interp = Interpreter::new();
    let stdin = io::stdin();

    println!("J interpreter (minimal)");
    println!("+/ i. 10  or  a =: i. 10  then  +/ a");
    println!("Ctrl+D to exit");
    println!();

    loop {
        print!("   ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,  // EOF
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() { continue; }

                match tokenize(input) {
                    Err(e) => println!("tokenize error: {}", e),
                    Ok(tokens) => {
                        match eval(&interp, &tokens) {
                            Ok(val)  => println!("{}", val),
                            Err(e)   => println!("error: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("read error: {}", e);
                break;
            }
        }
    }
}
