mod array;
mod error;
mod interp;
mod verbs;
mod eval;

use interp::Interpreter;
use eval::{tokenize, eval};
use std::io::{self, BufRead, Write};

fn run_line(interp: &mut Interpreter, input: &str) {
    // 소스 등록 후 source_id 획득
    // push 전 len이 곧 새 source의 인덱스
    let source_id = interp.sources.len();
    interp.push_source(input.to_string());

    match tokenize(input, source_id) {
        Err(e) => e.display(&interp.sources),
        Ok(tokens) => {
            match eval(interp, &tokens) {
                Ok(val) => {
                    // =: 는 결과를 출력하지 않음 (J 동작과 동일)
                    let is_assign = tokens.len() >= 2
                        && matches!(tokens[1].kind, eval::TokenKind::Assign);
                    if !is_assign {
                        println!("{}", val);
                    }
                }
                Err(e) => e.display(&interp.sources),
            }
        }
    }
}

fn main() {
    let mut interp = Interpreter::new();
    let stdin = io::stdin();

    println!("rj - J interpreter in Rust");
    println!("──────────────────────────");
    println!("  i. 5              NB. 0 1 2 3 4");
    println!("  +/ i. 10          NB. 45");
    println!("  mean =: +/ % #");
    println!("  mean i. 11        NB. 5");
    println!("  i. 2 3            NB. 2x3 matrix");
    println!("  $ i. 2 3          NB. 2 3");
    println!("  2 3 $ i. 6        NB. reshape");
    println!("  3 | 10            NB. 1  (10 mod 3)");
    println!("  | _5              NB. 5  (abs)");
    println!("Ctrl+D to exit");
    println!();

    loop {
        print!("   ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() { continue; }
                // NB. 주석 처리
                if input.starts_with("NB.") { continue; }
                run_line(&mut interp, input);
            }
            Err(e) => { eprintln!("read error: {}", e); break; }
        }
    }
}
