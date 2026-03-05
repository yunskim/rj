use std::fmt;
use std::sync::Arc;

/// J의 AT 필드에 해당
/// 명사뿐 아니라 동사/부사도 포함
#[derive(Debug, Clone, PartialEq)]
pub enum JType {
    Integer,
    Float,
    Verb,    // 동사 (+ - * % i. 등)
    Adverb,  // 부사 (/ \ 등)
}

/// Verb trait을 Arc로 감싼 타입
/// J의 A 블록에 저장되는 동사 표현
/// Arc: Clone 가능 + 멀티스레드 안전
pub type VerbBox = Arc<dyn crate::verbs::Verb>;

/// J의 A 블록에 해당
/// 명사와 동사 모두 JArray로 표현
#[derive(Clone)]
pub struct JArray {
    pub typ:   JType,
    pub rank:  usize,        // AR
    pub shape: Vec<usize>,   // AS
    pub count: usize,        // AN
    pub data:  JData,
}

#[derive(Clone)]
pub enum JData {
    Integer(Vec<i64>),
    Float(Vec<f64>),
    /// 동사: J의 fgh[0], fgh[1] 에 해당
    Verb(VerbBox),
}

/// J의 A 타입에 해당 - Arc로 usecount 자동 관리
pub type JVal = Arc<JArray>;

impl JArray {
    /// 정수 스칼라 생성
    pub fn scalar_int(n: i64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Integer,
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Integer(vec![n]),
        })
    }

    /// 정수 벡터 생성 (rank 1)
    pub fn vector_int(v: Vec<i64>) -> JVal {
        let n = v.len();
        Arc::new(JArray {
            typ:   JType::Integer,
            rank:  1,
            shape: vec![n],
            count: n,
            data:  JData::Integer(v),
        })
    }

    /// 동사를 JArray로 감싸기
    /// mean =: +/ % # 처럼 동사를 이름에 바인딩할 때 사용
    /// J에서 동사도 A 블록인 것과 동일한 원리
    pub fn from_verb(verb: VerbBox) -> JVal {
        Arc::new(JArray {
            typ:   JType::Verb,
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Verb(verb),
        })
    }

    /// 정수 데이터 접근
    pub fn as_int(&self) -> Option<&Vec<i64>> {
        match &self.data {
            JData::Integer(v) => Some(v),
            _ => None,
        }
    }

    /// 동사 데이터 접근
    /// J의 FAV(self) 에 해당
    pub fn as_verb(&self) -> Option<&VerbBox> {
        match &self.data {
            JData::Verb(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_verb(&self) -> bool {
        self.typ == JType::Verb
    }
}

impl fmt::Display for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            JData::Integer(v) => {
                if self.rank == 0 {
                    write!(f, "{}", v[0])
                } else {
                    let s: Vec<String> = v.iter().map(|x| x.to_string()).collect();
                    write!(f, "{}", s.join(" "))
                }
            }
            JData::Float(v) => {
                if self.rank == 0 {
                    write!(f, "{}", v[0])
                } else {
                    let s: Vec<String> = v.iter().map(|x| x.to_string()).collect();
                    write!(f, "{}", s.join(" "))
                }
            }
            JData::Verb(v) => write!(f, "(verb:{})", v.name()),
        }
    }
}

impl fmt::Debug for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JArray({:?})", self.typ)
    }
}
