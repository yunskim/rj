/// J 엔진 Rust 바인딩 사용 예시
///
/// 실행 방법:
///   cargo run --example basic -- /path/to/libj.so
///
/// Linux 기본 경로 예시:
///   cargo run --example basic -- /usr/lib/j9/libj.so
///   cargo run --example basic -- ~/.local/lib/j9/libj.so

use j_engine::JEngine;
use std::env;

fn main() {
    // 커맨드라인에서 라이브러리 경로를 받습니다.
    let lib_path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("사용법: basic <libj.so 경로>");
        eprintln!("예시:   basic /usr/lib/j9/libj.so");
        std::process::exit(1);
    });

    println!("=== J 엔진 로드 중: {} ===\n", lib_path);

    // ─── 1. 엔진 초기화 ───────────────────────────────────────────────────────
    // JInit() → JSM(callbacks) 순서로 자동 처리됩니다.
    let mut j = JEngine::load(&lib_path).unwrap_or_else(|e| {
        eprintln!("엔진 초기화 실패: {}", e);
        std::process::exit(1);
    });

    println!("엔진 초기화 성공.\n");

    // ─── 2. 기본 산술 ─────────────────────────────────────────────────────────
    run(&mut j, "1 + 1");
    run(&mut j, "2 ^ 10");
    run(&mut j, "+/ 1 2 3 4 5");        // 합계: 15
    run(&mut j, "i. 5");                 // 0 1 2 3 4

    // ─── 3. 이름 대입 ─────────────────────────────────────────────────────────
    run(&mut j, "x =: 42");
    run(&mut j, "x * 2");

    // ─── 4. 배열 연산 ─────────────────────────────────────────────────────────
    run(&mut j, "v =: 10 20 30 40 50");
    run(&mut j, "+/ v");                 // 합계: 150
    run(&mut j, "v % +/ v");            // 정규화

    // ─── 5. 명시적 정의 ───────────────────────────────────────────────────────
    run(&mut j, "double =: 3 : '2 * y'");
    run(&mut j, "double 7");

    // ─── 6. 암묵적 정의 (tacit) ───────────────────────────────────────────────
    run(&mut j, "mean =: +/ % #");
    run(&mut j, "mean 1 2 3 4 5");

    // ─── 7. 에러 처리 ─────────────────────────────────────────────────────────
    println!("--- 에러 예시 ---");
    match j.eval("1 % 0") {             // 0으로 나누기 (J는 inf 반환, 에러 없음)
        Ok(()) => println!("  결과: {}", j.last_output().trim()),
        Err(code) => println!("  에러 코드: {}", code),
    }
    match j.eval("undefined_name_xyz") { // 미정의 이름
        Ok(()) => println!("  결과: {}", j.last_output().trim()),
        Err(code) => println!("  에러 코드: {} (출력: {})", code, j.last_output().trim()),
    }

    println!("\n=== 완료 ===");
    // Drop 시 JFree()가 자동 호출됩니다.
}

/// J 문장을 실행하고 결과를 출력하는 헬퍼 함수
fn run(j: &mut JEngine, sentence: &str) {
    match j.eval(sentence) {
        Ok(()) => {
            let output = j.last_output();
            let output = output.trim();
            if output.is_empty() {
                println!("  {:30} → (출력 없음)", sentence);
            } else {
                println!("  {:30} → {}", sentence, output);
            }
        }
        Err(code) => {
            println!(
                "  {:30} → 에러({}): {}",
                sentence,
                code,
                j.last_output().trim()
            );
        }
    }
}
