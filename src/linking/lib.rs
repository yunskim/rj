//! J 엔진 Rust 바인딩
//!
//! J 엔진 DLL/SO의 공개 API를 Rust에서 안전하게 사용할 수 있도록 래핑합니다.
//!
//! # 사용 예시
//! ```rust,no_run
//! use j_engine::JEngine;
//!
//! let mut engine = JEngine::load("/usr/lib/j9/libj.so").unwrap();
//! engine.eval("1 + 1").unwrap();
//! println!("{}", engine.last_output());
//! ```

use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// C API 타입 정의
//
// jsrc/io.c 의 공개 API 시그니처를 그대로 옮깁니다.
//
//   typedef void  (*outputtype)(J, int type, C* string);
//   typedef C*    (*inputtype) (J, C* prompt);
//   typedef int   (*dowdtype)  (J, int t, A w, A* z);
//   CDPROC JS     JInit(void);
//   CDPROC int    JDo(JS jt, C* lp);
//   CDPROC void   JSM(JS jt, void* callbacks[]);
//   CDPROC int    JFree(JS jt);
//   CDPROC C*     JGetR(JS jt);
// ─────────────────────────────────────────────────────────────────────────────

/// J 엔진의 불투명 인스턴스 포인터 (JST*)
/// 내부 구조는 노출하지 않고 포인터로만 전달합니다.
#[repr(C)]
pub struct JInstance {
    _opaque: [u8; 0],
}
pub type JPtr = *mut JInstance;

// J 엔진이 JSM에 전달받는 콜백 함수 타입들
//
// `extern "C"` 와 `unsafe` 가 필요합니다.
// J 엔진은 이 함수를 C 호출 규약으로 직접 호출합니다.

/// 출력 콜백: J가 결과를 출력할 때 호출됩니다.
/// - `jt`  : J 인스턴스 포인터 (무시해도 됩니다)
/// - `type`: 출력 종류 (1=결과, 2=에러, 3=로그, 5=종료)
/// - `s`   : 출력할 NUL 종결 C 문자열
pub type OutputFn = unsafe extern "C" fn(jt: JPtr, output_type: c_int, s: *const c_char);

/// 입력 콜백: J가 키보드 입력을 요청할 때 호출됩니다.
/// - `jt`    : J 인스턴스 포인터
/// - `prompt`: 프롬프트 문자열 (빈 문자열이면 무음 요청)
/// - 반환값  : 사용자 입력 문자열 (static 또는 충분히 오래 사는 버퍼)
pub type InputFn = unsafe extern "C" fn(jt: JPtr, prompt: *const c_char) -> *const c_char;

/// wd 콜백: J에서 `11!:x`(window driver)를 실행할 때 호출됩니다.
/// 간단한 용도에서는 NULL을 전달해도 됩니다 (wd를 쓰지 않는 경우).
pub type WdFn = unsafe extern "C" fn(
    jt: JPtr,
    t: c_int,
    w: *mut c_void,
    z: *mut *mut c_void,
) -> c_int;

// ─────────────────────────────────────────────────────────────────────────────
// 출력 수집을 위한 전역 버퍼
//
// J 엔진의 출력 콜백은 C 함수 포인터이므로 클로저를 쓸 수 없습니다.
// 전역 Mutex<String>으로 출력을 수집하고, eval() 후에 꺼내 씁니다.
// ─────────────────────────────────────────────────────────────────────────────

static OUTPUT_BUFFER: Mutex<String> = Mutex::new(String::new());

/// J 엔진이 결과를 출력할 때 호출되는 C 콜백 함수입니다.
/// 출력 문자열을 전역 버퍼에 누적합니다.
///
/// # Safety
/// J 엔진이 C 호출 규약으로 직접 호출하므로 `unsafe extern "C"` 가 필요합니다.
pub unsafe extern "C" fn j_output_callback(
    _jt: JPtr,
    output_type: c_int,
    s: *const c_char,
) {
    if s.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy();
    let mut buf = OUTPUT_BUFFER.lock().unwrap();
    match output_type {
        1 => buf.push_str(&text),           // MTYOFM: 일반 결과
        2 => {
            buf.push_str("[ERROR] ");        // MTYOER: 에러
            buf.push_str(&text);
        }
        5 => {}                              // MTYOEXIT: 종료 신호, 무시
        _ => buf.push_str(&text),
    }
}

/// J 엔진이 입력을 요청할 때 호출되는 C 콜백 함수입니다.
/// 기본 구현은 빈 문자열을 반환하여 입력 없음을 알립니다.
///
/// # Safety
/// J 엔진이 C 호출 규약으로 직접 호출하므로 `unsafe extern "C"` 가 필요합니다.
pub unsafe extern "C" fn j_input_callback(
    _jt: JPtr,
    _prompt: *const c_char,
) -> *const c_char {
    // 빈 C 문자열의 static 포인터를 반환합니다.
    // 실제 REPL 구현이라면 stdin에서 읽어야 합니다.
    b"\0".as_ptr() as *const c_char
}

// ─────────────────────────────────────────────────────────────────────────────
// J 엔진 래퍼 구조체
// ─────────────────────────────────────────────────────────────────────────────

/// J 엔진 인스턴스를 래핑하는 안전한 Rust 구조체입니다.
///
/// `Drop`을 구현하여 소멸 시 `JFree()`를 자동으로 호출합니다.
pub struct JEngine {
    /// dlopen으로 로드된 공유 라이브러리
    _lib: Library,

    /// J 엔진 인스턴스 포인터 (JST*)
    jt: JPtr,

    // 함수 포인터들을 캐싱합니다.
    // Library보다 먼저 드롭되어서는 안 되므로 raw pointer로 보관합니다.
    fn_jdo: unsafe extern "C" fn(JPtr, *const c_char) -> c_int,
    fn_jfree: unsafe extern "C" fn(JPtr) -> c_int,
    fn_jgetr: unsafe extern "C" fn(JPtr) -> *const c_char,
}

impl JEngine {
    /// J 엔진 공유 라이브러리를 로드하고 새 인스턴스를 초기화합니다.
    ///
    /// # 인수
    /// - `lib_path`: 라이브러리 경로
    ///   - Linux:   `/usr/lib/j9/libj.so` 또는 `~/.local/lib/j9/libj.so`
    ///   - macOS:   `/Applications/j9.5/libj.dylib`
    ///   - Windows: `C:\j9\bin\j.dll`
    ///
    /// # 오류
    /// 라이브러리 로드 실패, 심볼 조회 실패, JInit 실패 시 오류를 반환합니다.
    pub fn load(lib_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // 1. 공유 라이브러리 로드
        let lib = unsafe { Library::new(lib_path) }
            .map_err(|e| format!("라이브러리 로드 실패 '{}': {}", lib_path, e))?;

        // 2. 필요한 심볼들을 조회합니다.
        //    Symbol<T>은 Library의 생명주기에 묶여 있으므로
        //    raw pointer로 변환해서 보관합니다.
        let fn_jinit: Symbol<unsafe extern "C" fn() -> JPtr> =
            unsafe { lib.get(b"JInit\0") }
                .map_err(|e| format!("JInit 심볼 없음: {}", e))?;

        let fn_jsm: Symbol<unsafe extern "C" fn(JPtr, *mut *mut c_void)> =
            unsafe { lib.get(b"JSM\0") }
                .map_err(|e| format!("JSM 심볼 없음: {}", e))?;

        let fn_jdo: Symbol<unsafe extern "C" fn(JPtr, *const c_char) -> c_int> =
            unsafe { lib.get(b"JDo\0") }
                .map_err(|e| format!("JDo 심볼 없음: {}", e))?;
        let fn_jdo = unsafe { *fn_jdo.into_raw() };

        let fn_jfree: Symbol<unsafe extern "C" fn(JPtr) -> c_int> =
            unsafe { lib.get(b"JFree\0") }
                .map_err(|e| format!("JFree 심볼 없음: {}", e))?;
        let fn_jfree = unsafe { *fn_jfree.into_raw() };

        let fn_jgetr: Symbol<unsafe extern "C" fn(JPtr) -> *const c_char> =
            unsafe { lib.get(b"JGetR\0") }
                .map_err(|e| format!("JGetR 심볼 없음: {}", e))?;
        let fn_jgetr = unsafe { *fn_jgetr.into_raw() };

        // 3. J 인스턴스 생성
        let jt = unsafe { fn_jinit() };
        if jt.is_null() {
            return Err("JInit() 가 NULL을 반환했습니다. 라이브러리 초기화 실패.".into());
        }

        // 4. JSM으로 콜백 등록
        //
        // callbacks 배열은 io.c 주석에 정의된 순서를 따릅니다:
        //   [0] = output 함수 포인터
        //   [1] = wd 함수 포인터 (NULL = wd 미지원)
        //   [2] = input 함수 포인터
        //   [3] = poll 함수 포인터 (NULL)
        //   [4] = sm 타입 + 옵션 (SMCON=3)
        //
        // SMCON(3) = jconsole 모드로 설정합니다.
        const SM_CON: usize = 3; // #define SMCON 3

        let mut callbacks: [*mut c_void; 5] = [
            j_output_callback as *mut c_void,   // [0] smoutput
            std::ptr::null_mut(),               // [1] smdowd (wd 미사용)
            j_input_callback as *mut c_void,    // [2] sminput
            std::ptr::null_mut(),               // [3] smpoll
            SM_CON as *mut c_void,              // [4] sm type = SMCON
        ];

        unsafe { fn_jsm(jt, callbacks.as_mut_ptr() as *mut *mut c_void) };

        Ok(JEngine {
            _lib: lib,
            jt,
            fn_jdo,
            fn_jfree,
            fn_jgetr,
        })
    }

    /// J 문장을 실행합니다.
    ///
    /// # 인수
    /// - `sentence`: 실행할 J 문장 (예: `"1 + 1"`, `"echo 'hello'"`)
    ///
    /// # 반환
    /// - `Ok(())`: 에러 없이 실행 완료
    /// - `Err(code)`: JDo가 비영(non-zero) 오류 코드를 반환한 경우
    pub fn eval(&mut self, sentence: &str) -> Result<(), c_int> {
        // 전역 출력 버퍼를 비웁니다.
        OUTPUT_BUFFER.lock().unwrap().clear();

        // Rust 문자열 → NUL 종결 C 문자열
        let c_sentence = CString::new(sentence)
            .expect("문장에 NUL 바이트가 포함될 수 없습니다");

        let ret = unsafe { (self.fn_jdo)(self.jt, c_sentence.as_ptr()) };

        if ret == 0 {
            Ok(())
        } else {
            Err(ret)
        }
    }

    /// 마지막 `eval()` 호출에서 J 엔진이 출력한 문자열을 반환합니다.
    ///
    /// `JGetR()`을 사용하지 않고 콜백으로 수집한 버퍼를 씁니다.
    /// (JGetR은 capture 모드에서만 동작하므로 콜백 방식이 더 안정적입니다.)
    pub fn last_output(&self) -> String {
        OUTPUT_BUFFER.lock().unwrap().clone()
    }

    /// `JGetR()`로 캡처된 출력을 가져옵니다.
    ///
    /// smoutput 콜백 대신 내부 capture 버퍼를 쓸 때 유용합니다.
    pub fn get_captured(&self) -> String {
        let ptr = unsafe { (self.fn_jgetr)(self.jt) };
        if ptr.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    /// J 인스턴스 포인터를 반환합니다. (고급 사용)
    pub fn raw_ptr(&self) -> JPtr {
        self.jt
    }
}

impl Drop for JEngine {
    fn drop(&mut self) {
        // JFree()로 엔진 내부 리소스를 해제합니다.
        unsafe { (self.fn_jfree)(self.jt) };
    }
}

// JEngine은 단일 스레드에서만 사용해야 합니다.
// JDo()는 재진입이 불가능하므로(RECSTATERUNNING 체크) Send는 허용하지 않습니다.
// 필요하다면 Mutex<JEngine>으로 감싸서 쓰세요.
