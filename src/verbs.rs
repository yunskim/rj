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
/// monad i. n       → 0 1 2 ... n-1          (rank 1)
/// monad i. 2 3     → 2x3 행렬 0..5          (rank 2)
/// monad i. 2 3 4   → 2x3x4 배열 0..23       (rank 3)
pub struct Iota;

impl Verb for Iota {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        match w.as_int() {
            Some(v) => {
                // 스칼라: i. 10 → 0 1 2 ... 9
                if w.rank == 0 {
                    let n = v[0];
                    if n < 0 {
                        return Err("domain error: i. requires non-negative integer".to_string());
                    }
                    return Ok(JArray::vector_int((0..n).collect()));
                }

                // 벡터: i. 2 3 → 2x3 행렬
                // shape의 각 원소가 각 차원 크기
                if w.rank == 1 {
                    let shape: Vec<usize> = v.iter().map(|&x| {
                        if x < 0 { 0 } else { x as usize }
                    }).collect();
                    let count: usize = shape.iter().product::<usize>().max(1);
                    let data: Vec<i64> = (0..count as i64).collect();
                    return Ok(JArray::array_int(shape, data));
                }

                Err("domain error: i. requires scalar or vector".to_string())
            }
            _ => Err("domain error: i. requires integer argument".to_string()),
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

/// % (divide)
/// monad %: reciprocal
/// dyad %: divide
pub struct Percent;

impl Verb for Percent {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        // % w → 1 % w (역수)
        match w.as_int() {
            Some(v) if w.rank == 0 => {
                if v[0] == 0 { return Err("domain error: divide by zero".to_string()); }
                // 정수 역수는 float이어야 하지만 단순화를 위해 정수 나눗셈
                Ok(JArray::scalar_int(1 / v[0]))
            }
            _ => Err("domain error: % requires scalar".to_string()),
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String> {
        // a % w → a 나누기 w
        match (a.as_int(), w.as_int()) {
            (Some(av), Some(wv)) => {
                if av.len() != wv.len() && av.len() != 1 && wv.len() != 1 {
                    return Err("length error".to_string());
                }
                let result: Vec<i64> = if av.len() == wv.len() {
                    av.iter().zip(wv.iter()).map(|(a, b)| {
                        if *b == 0 { 0 } else { a / b }
                    }).collect()
                } else if av.len() == 1 {
                    wv.iter().map(|b| if *b == 0 { 0 } else { av[0] / b }).collect()
                } else {
                    av.iter().map(|a| if wv[0] == 0 { 0 } else { a / wv[0] }).collect()
                };
                if result.len() == 1 {
                    Ok(JArray::scalar_int(result[0]))
                } else {
                    Ok(JArray::vector_int(result))
                }
            }
            _ => Err("domain error: % requires integers".to_string()),
        }
    }

    fn name(&self) -> &str { "%" }
}

/// # (tally / copy)
/// monad #: leading axis 크기
///   # 1 2 3     → 3
///   # i. 2 3    → 2  (행 수)
///   # i. 2 3 4  → 2  (최외곽 차원)
pub struct Hash;

impl Verb for Hash {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        Ok(JArray::scalar_int(w.tally() as i64))
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> Result<JVal, String> {
        Err("# dyad not implemented".to_string())
    }

    fn name(&self) -> &str { "#" }
}

/// / (slash) - adverb
/// u/ w → w의 원소들을 u로 fold
pub struct Slash {
    pub u: VerbBox,   // J의 FAV(self)->fgh[0] 에 해당
}

impl Verb for Slash {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
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

/// Fork - Derived Verb
/// J의 가장 핵심적인 tacit 패턴
///
/// monad fork: (f g h) w = (f w) g (h w)
/// dyad  fork: (f g h) a w = (a f w) g (a h w)
///
/// 예: mean =: +/ % #
///     mean 1 2 3 4 5
///     = (+/ 1 2 3 4 5) % (# 1 2 3 4 5)
///     = 15 % 5
///     = 3
///
/// J 소스의 jtfork()에 해당
/// fgh[0]=f, fgh[1]=g, fgh[2]=h 구조와 동일
pub struct Fork {
    pub f: VerbBox,   // fgh[0]: 왼쪽 동사
    pub g: VerbBox,   // fgh[1]: 가운데 동사 (결합)
    pub h: VerbBox,   // fgh[2]: 오른쪽 동사
}

impl Verb for Fork {
    /// monad: (f g h) w = (f w) g (h w)
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        let fw = self.f.monad(interp, w)?;   // f w
        let hw = self.h.monad(interp, w)?;   // h w
        self.g.dyad(interp, &fw, &hw)        // (f w) g (h w)
    }

    /// dyad: (f g h) a w = (a f w) g (a h w)
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String> {
        let fw = self.f.dyad(interp, a, w)?; // a f w
        let hw = self.h.dyad(interp, a, w)?; // a h w
        self.g.dyad(interp, &fw, &hw)        // (a f w) g (a h w)
    }

    fn name(&self) -> &str { "fork" }
}

