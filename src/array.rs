use std::fmt;
use std::sync::Arc;

/// J의 AT 필드에 해당
#[derive(Debug, Clone, PartialEq)]
pub enum JType {
    Integer,
    Float,
}

/// J의 A 블록에 해당
#[derive(Debug, Clone)]
pub struct JArray {
    pub typ:   JType,
    pub rank:  usize,        // AR
    pub shape: Vec<usize>,   // AS
    pub count: usize,        // AN
    pub data:  JData,
}

#[derive(Debug, Clone)]
pub enum JData {
    Integer(Vec<i64>),
    Float(Vec<f64>),
}

/// Arc로 감싸서 usecount 자동 관리
/// J의 A 타입에 해당
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

    /// 정수 데이터 접근
    pub fn as_int(&self) -> Option<&Vec<i64>> {
        match &self.data {
            JData::Integer(v) => Some(v),
            _ => None,
        }
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
        }
    }
}
