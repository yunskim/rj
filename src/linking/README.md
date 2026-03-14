# J 엔진 Rust 바인딩

J 엔진 공유 라이브러리(libj.so / j.dll)를 Rust에서 호출하는 바인딩입니다.

## 파일 구조

```
j_engine/
├── Cargo.toml          # 의존성 (libloading)
├── src/
│   └── lib.rs          # 바인딩 핵심 코드
└── examples/
    └── basic.rs        # 사용 예시
```

## 설치 및 실행

```bash
# 의존성 설치
cargo build

# 예시 실행 (libj.so 경로를 실제 경로로 변경)
cargo run --example basic -- /usr/lib/j9/libj.so

# Linux에서 J 설치 후 기본 경로
# ~/.local/lib/j9/libj.so  또는  /usr/lib/j9/libj.so
```

## 핵심 구조 설명

### 1. 초기화 흐름

```
JEngine::load(path)
  │
  ├─ Library::new(path)          // dlopen / LoadLibrary
  ├─ lib.get("JInit")            // 심볼 로드
  ├─ lib.get("JSM")
  ├─ lib.get("JDo")
  ├─ lib.get("JFree")
  ├─ lib.get("JGetR")
  │
  ├─ JInit()                     // J 인스턴스 생성 → jt 획득
  │
  └─ JSM(jt, callbacks)          // 콜백 등록
       [0] j_output_callback     // J 출력 시 호출
       [1] NULL                  // wd 미사용
       [2] j_input_callback      // J 입력 요청 시 호출
       [3] NULL                  // poll 미사용
       [4] SMCON(3)              // jconsole 모드
```

### 2. 콜백 등록의 의미

J 엔진은 라이브러리이므로 직접 화면에 출력하거나 키보드를 읽지 못합니다.
`JSM()`으로 "출력이 필요하면 이 함수를 불러라", "입력이 필요하면 이 함수를 불러라"고
함수 포인터를 등록합니다.

```
  [Rust 프론트엔드]                [J 엔진 라이브러리]
        │
        │  JSM(jt, [output_fn, ...])
        ├────────────────────────────►  jt->smoutput = output_fn
        │                               jt->sminput  = input_fn
        │
        │  JDo(jt, "1 + 1")
        ├────────────────────────────►  계산...
        │                               결과 출력 필요
        │  j_output_callback(jt,1,"2")
        ◄────────────────────────────   jt->smoutput(jt, 1, "2")
        │
        │  (버퍼에 "2" 저장)
```

### 3. 출력 수집 방식

C 함수 포인터는 클로저를 받을 수 없으므로, `static Mutex<String>`에 출력을 누적합니다.
`eval()` 호출 전에 버퍼를 비우고, 호출 후 `last_output()`으로 가져옵니다.

```rust
// 내부 동작
pub unsafe extern "C" fn j_output_callback(_jt: JPtr, type: c_int, s: *const c_char) {
    let text = CStr::from_ptr(s).to_string_lossy();
    OUTPUT_BUFFER.lock().unwrap().push_str(&text);
}
```

### 4. 주의사항

- `JEngine`은 `Send`가 아닙니다. `JDo()`는 재진입 불가이므로 멀티스레드 환경에서는
  `Mutex<JEngine>`으로 감싸야 합니다.
- `Drop`이 `JFree()`를 자동 호출하므로 수동 해제 불필요합니다.
- J 엔진은 AVX 명령을 사용하므로, 엔진 호출 전후에 SSE 상태가 변할 수 있습니다
  (io.c의 ZEROUPPER 주석 참조). 일반적인 사용에서는 문제없습니다.
