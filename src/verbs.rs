use crate::array::{JArray, JData, JVal};
use crate::interp::Interpreter;

/// J의 모든 동사가 구현하는 trait
/// DF1, DF2 매크로에 해당
pub trait Verb: Send + Sync {
    /// 단항 (monad): w만 있음
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String>;
    /// 이항 (dyad): a, w 둘 다 있음
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String>;
    fn name(&self) -> &str;
}

/// i. (iota) - 0부터 n-1까지 정수 배열 생성
pub struct Iota;

impl Verb for Iota {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        match w.as_int() {
            Some(v) if w.rank == 0 => {
                let n = v[0];
                if n < 0 {
                    return Err("domain error: i. requires non-negative integer".to_string());
                }
                Ok(JArray::vector_int((0..n).collect()))
            }
            _ => Err("domain error: i. requires scalar integer".to_string()),
        }
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> Result<JVal, String> {
        Err("i. dyad not implemented".to_string())
    }

    fn name(&self) -> &str { "i." }
}

/// + (plus) - 덧셈
pub struct Plus;

impl Verb for Plus {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        // monad +: conjugate (실수에서는 identity)
        Ok(Arc::clone(w))
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String> {
        match (a.as_int(), w.as_int()) {
            (Some(av), Some(wv)) => {
                if av.len() != wv.len() && av.len() != 1 && wv.len() != 1 {
                    return Err("length error".to_string());
                }
                let result: Vec<i64> = if av.len() == wv.len() {
                    av.iter().zip(wv.iter()).map(|(a, b)| a + b).collect()
                } else if av.len() == 1 {
                    wv.iter().map(|b| av[0] + b).collect()
                } else {
                    av.iter().map(|a| a + wv[0]).collect()
                };
                if result.len() == 1 {
                    Ok(JArray::scalar_int(result[0]))
                } else {
                    Ok(JArray::vector_int(result))
                }
            }
            _ => Err("domain error: + requires integers".to_string()),
        }
    }

    fn name(&self) -> &str { "+" }
}

/// / (slash) - adverb: 동사를 받아서 insert 동사를 만듦
/// +/ → PlusReduce, */ → TimesReduce 등
pub struct Slash {
    pub u: Box<dyn Verb>,   // J의 FAV(self)->fgh[0] 에 해당
}

impl Verb for Slash {
    /// +/ w → w의 원소들을 u로 fold
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        match w.as_int() {
            Some(v) => {
                if v.is_empty() {
                    return Err("domain error: empty array".to_string());
                }
                // 오른쪽에서 왼쪽으로 fold (J의 insert 동작)
                let mut result = JArray::scalar_int(*v.last().unwrap());
                for &x in v.iter().rev().skip(1) {
                    let left = JArray::scalar_int(x);
                    result = self.u.dyad(interp, &left, &result)?;
                }
                Ok(result)
            }
            _ => Err("domain error: / requires integer array".to_string()),
        }
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> Result<JVal, String> {
        Err("/ dyad not implemented".to_string())
    }

    fn name(&self) -> &str { "/" }
}

// Arc를 사용하기 위해 필요
use std::sync::Arc;
