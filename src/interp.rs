use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::array::JVal;

/// JST에 해당 - 모든 스레드가 공유
pub struct SharedState {
    pub global: RwLock<HashMap<String, JVal>>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState { global: RwLock::new(HashMap::new()) }
    }

    pub fn assign(&self, name: String, val: JVal) {
        self.global.write().unwrap().insert(name, val);
    }

    pub fn lookup(&self, name: &str) -> Option<JVal> {
        self.global.read().unwrap().get(name).cloned()
    }
}

/// JTT에 해당 - 스레드마다 독립
pub struct ThreadState {
    pub locsyms: HashMap<String, JVal>,
}

impl ThreadState {
    pub fn new() -> Self {
        ThreadState { locsyms: HashMap::new() }
    }
}

/// 전체 인터프리터 컨텍스트
/// 실행 엔진만 담당 - 소스 관리는 프론트엔드 책임
pub struct Interpreter {
    pub shared: Arc<SharedState>,
    pub thread: ThreadState,
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            shared: Arc::new(SharedState::new()),
            thread: ThreadState::new(),
        }
    }

    /// 이름 조회: 지역 → 전역
    pub fn lookup(&self, name: &str) -> Option<JVal> {
        if let Some(val) = self.thread.locsyms.get(name) {
            return Some(Arc::clone(val));
        }
        self.shared.lookup(name)
    }

    /// 전역 바인딩 (=:)
    pub fn assign_global(&self, name: String, val: JVal) {
        self.shared.assign(name, val);
    }
}
