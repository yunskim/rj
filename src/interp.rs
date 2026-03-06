use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::array::JVal;

/// JST에 해당 - 모든 스레드가 공유
pub struct SharedState {
    /// 전역 심볼 테이블 (=: 로 바인딩된 이름들)
    /// locale 추가 시 HashMap<String, HashMap<String, JVal>> 로 확장
    pub global: RwLock<HashMap<String, JVal>>,

    /// locale 간 이름 탐색 순서
    /// locale 자체가 공유 자원이므로 탐색 순서도 공유
    /// 예: "mylib" → ["mylib", "base"]
    pub search_path: RwLock<HashMap<String, Vec<String>>>,
}

impl SharedState {
    pub fn new() -> Self {
        SharedState {
            global:      RwLock::new(HashMap::new()),
            search_path: RwLock::new(HashMap::new()),
        }
    }

    pub fn assign(&self, name: String, val: JVal) {
        self.global.write().unwrap().insert(name, val);
    }

    pub fn lookup(&self, name: &str) -> Option<JVal> {
        self.global.read().unwrap().get(name).cloned()
    }
}

/// JTT에 해당 - 스레드마다 독립
/// locale 자체는 SharedState에 있고
/// 스레드마다 다른 것은 "지금 어느 locale에서 실행 중인가" 뿐
pub struct ThreadState {
    /// 지역 심볼 테이블 (=. 로 바인딩된 이름들)
    pub locsyms: HashMap<String, JVal>,

    /// 현재 실행 중인 locale 이름
    /// locale 추가 시 사용 - 지금은 항상 "base"
    pub current_locale: String,
}

impl ThreadState {
    pub fn new() -> Self {
        ThreadState {
            locsyms:        HashMap::new(),
            current_locale: "base".to_string(),
        }
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
