use crate::array::{JArray, JData, JVal, VerbBox};
use crate::interp::Interpreter;
use std::sync::Arc;

/// J의 모든 동사가 구현하는 trait
/// Send + Sync: 멀티스레드 안전 (t. T. 지원을 위해)
pub trait Verb: Send + Sync {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String>;
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String>;
    fn name(&self) -> &str;
}

/// i. (iota)
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

/// + (plus)
pub struct Plus;

impl Verb for Plus {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
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

/// / (slash) - adverb
/// u/ w → w의 원소들을 u로 fold
/// J의 fgh[0]에 해당하는 u를 VerbBox로 보관
/// → JArray::from_verb()로 감싸서 심볼 테이블에 저장 가능
pub struct Slash {
    pub u: VerbBox,   // J의 FAV(self)->fgh[0] 에 해당
}

impl Verb for Slash {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        // w가 동사인 경우: mean =: +/ 처럼 동사가 인자로 올 때
        // (현재는 정수 배열만 처리)
        match w.as_int() {
            Some(v) => {
                if v.is_empty() {
                    return Err("domain error: empty array".to_string());
                }
                // 오른쪽에서 왼쪽으로 fold
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

