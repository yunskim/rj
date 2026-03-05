use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::array::JVal;

/// JST에 해당 - 모든 스레드가 공유
pub struct SharedState {
    /// 전역 심볼 테이블 (=: 로 바인딩된 이름들)
    /// RwLock: 읽기는 동시에, 쓰기는 독점
    pub global: RwLock<HashMap<String, JVal>>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState {
            global: RwLock::new(HashMap::new()),
        }
    }

    /// 이름 바인딩 (=:)
    pub fn assign(&self, name: String, val: JVal) {
        self.global.write().unwrap().insert(name, val);
    }

    /// 이름 조회
    pub fn lookup(&self, name: &str) -> Option<JVal> {
        self.global.read().unwrap().get(name).cloned()
    }
}

/// JTT에 해당 - 스레드마다 독립
pub struct ThreadState {
    /// 지역 심볼 테이블
    pub locsyms: HashMap<String, JVal>,
    /// 에러 상태 (J의 jerr)
    pub jerr: Option<String>,
}

impl ThreadState {
    pub fn new() -> Self {
        ThreadState {
            locsyms: HashMap::new(),
            jerr: None,
        }
    }
}

/// 전체 인터프리터 컨텍스트
/// 모든 동사 함수에 &Interpreter 로 전달됨
pub struct Interpreter {
    pub shared: Arc<SharedState>,  // 공유 상태, Arc로 역참조 1번
    pub thread: ThreadState,       // 스레드 전용 상태
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            shared: Arc::new(SharedState::new()),
            thread: ThreadState::new(),
        }
    }

    /// 이름 조회: 지역 → 전역 순서
    /// J의 심볼 조회 순서와 동일
    pub fn lookup(&self, name: &str) -> Option<JVal> {
        // 먼저 지역 심볼 테이블 확인
        if let Some(val) = self.thread.locsyms.get(name) {
            return Some(Arc::clone(val));
        }
        // 없으면 전역 심볼 테이블 확인
        self.shared.lookup(name)
    }

    /// 전역 바인딩 (=:)
    pub fn assign_global(&self, name: String, val: JVal) {
        self.shared.assign(name, val);
    }
}
