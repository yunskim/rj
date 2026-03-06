mod array;
mod error;
mod gpu;
mod interp;
mod verbs;
mod eval;

use interp::Interpreter;
use eval::{tokenize, eval};
use std::io::{self, BufRead, Write};

/// 결과 출력
/// GPU 배열이면 CPU로 먼저 내려받은 후 출력
/// "출력 시에만 CPU로" 원칙
fn print_result(interp: &Interpreter, val: &array::JVal) {
    #[cfg(feature = "gpu")]
    if let gpu::Backend::Gpu(dev) = &interp.backend {
        if val.is_on_gpu() {
            let cpu_val = val.to_cpu(dev);
            println!("{}", cpu_val);
            return;
        }
    }
    println!("{}", val);
}

fn run_line(interp: &mut Interpreter, sources: &mut Vec<String>, input: &str) {
    let source_id = sources.len();
    sources.push(input.to_string());

    match tokenize(input, source_id) {
        Err(e) => e.display(sources),
        Ok(tokens) => {
            match eval(interp, &tokens) {
                Ok(val) => {
                    let is_assign = tokens.len() >= 2
                        && matches!(tokens[1].kind, eval::TokenKind::Assign);
                    if !is_assign {
                        print_result(interp, &val);
                    }
                }
                Err(e) => e.display(sources),
            }
        }
    }
}

fn main() {
    // --gpu 플래그로 GPU 백엔드 선택
    let use_gpu = std::env::args().any(|a| a == "--gpu");

    let mut interp = if use_gpu {
        #[cfg(feature = "gpu")]
        {
            println!("Initializing GPU backend...");
            Interpreter::new_with_gpu()
        }
        #[cfg(not(feature = "gpu"))]
        {
            eprintln!("GPU support not compiled in. Build with --features gpu");
            Interpreter::new()
        }
    } else {
        Interpreter::new()
    };

    let backend_name = match &interp.backend {
        gpu::Backend::Cpu => "CPU",
        #[cfg(feature = "gpu")]
        gpu::Backend::Gpu(_) => "GPU (wgpu)",
    };

    let mut sources = Vec::new();
    let stdin = io::stdin();

    println!("rj - J interpreter in Rust  [backend: {}]", backend_name);
    println!("──────────────────────────────────────────");
    println!("  i. 5              NB. 0 1 2 3 4");
    println!("  +/ i. 10          NB. 45");
    println!("  mean =: +/ % #");
    println!("  mean i. 11        NB. 5");
    println!("  1.5 + 2.5         NB. 4.0  (float)");
    println!("  3j4 * 1j2         NB. complex multiply");
    println!("  | 3j4             NB. 5.0  (magnitude)");
    println!("  + 3j4             NB. 3j_4 (conjugate)");
    println!("  1 2.5 3           NB. float vector (mixed promotes)");
    println!();
    if use_gpu {
        println!("  GPU 경로: +, -, *, % 는 VRAM에서 연산");
        println!("  중간 결과가 VRAM에 머묾 (전송 최소화)");
        println!("  출력 시에만 GPU→CPU 전송 발생");
    }
    println!("Ctrl+D to exit");
    println!();

    loop {
        print!("   ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0)  => break,
            Ok(_)  => {
                let input = line.trim();
                if input.is_empty() { continue; }
                if input.starts_with("NB.") { continue; }
                run_line(&mut interp, &mut sources, input);
            }
            Err(e) => { eprintln!("read error: {}", e); break; }
        }
    }
}
