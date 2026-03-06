use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::array::JVal;
use crate::gpu::Backend;

pub struct SharedState {
    pub global:      RwLock<HashMap<String, JVal>>,
    /// locale 간 이름 탐색 순서 (locale 추가 시 사용)
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

pub struct ThreadState {
    pub locsyms:        HashMap<String, JVal>,
    /// 현재 실행 중인 locale ("base" 가 기본값)
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
///
/// backend: CPU 또는 GPU 선택
/// GPU 초기화 실패 시 자동으로 CPU 폴백
pub struct Interpreter {
    pub shared:  Arc<SharedState>,
    pub thread:  ThreadState,
    /// 연산 백엔드 - rank1ex/rank2ex에 전달
    pub backend: Backend,
}

impl Interpreter {
    /// CPU 백엔드로 초기화
    pub fn new() -> Self {
        Interpreter {
            shared:  Arc::new(SharedState::new()),
            thread:  ThreadState::new(),
            backend: Backend::Cpu,
        }
    }

    /// GPU 백엔드로 초기화 시도
    /// GPU 사용 불가 시 CPU 폴백
    #[cfg(feature = "gpu")]
    pub fn new_with_gpu() -> Self {
        let backend = crate::gpu::GpuDevice::try_new()
            .map(Backend::Gpu)
            .unwrap_or_else(|| {
                eprintln!("GPU initialization failed, falling back to CPU");
                Backend::Cpu
            });
        Interpreter {
            shared:  Arc::new(SharedState::new()),
            thread:  ThreadState::new(),
            backend,
        }
    }

    pub fn lookup(&self, name: &str) -> Option<JVal> {
        if let Some(val) = self.thread.locsyms.get(name) {
            return Some(Arc::clone(val));
        }
        self.shared.lookup(name)
    }

    pub fn assign_global(&self, name: String, val: JVal) {
        self.shared.assign(name, val);
    }
}
