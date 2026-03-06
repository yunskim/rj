use crate::array::{JArray, JVal, VerbBox};
use crate::error::{JError, JErrorKind, JResult};
use crate::interp::Interpreter;
use std::sync::Arc;

/// J의 모든 동사가 구현하는 trait
/// Send + Sync: 멀티스레드 안전
pub trait Verb: Send + Sync {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal>;
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal>;
    fn name(&self) -> &str;
}

// ─────────────────────────────────────────
// 에러 생성 헬퍼
// span 없이 에러를 만드는 경우가 많으므로
// 동사 내부에서는 no_loc를 기본으로 사용
// 호출자(eval)가 span을 붙여줄 수 있음
// ─────────────────────────────────────────

fn domain_err(msg: impl Into<String>) -> JError {
    JError::no_loc(JErrorKind::Domain, msg)
}

fn length_err(msg: impl Into<String>) -> JError {
    JError::no_loc(JErrorKind::Length, msg)
}

fn rank_err(msg: impl Into<String>) -> JError {
    JError::no_loc(JErrorKind::Rank, msg)
}

// ─────────────────────────────────────────
// 공통 헬퍼: 두 정수 배열에 이항 연산 적용
// scalar extension 처리 포함
// ─────────────────────────────────────────

fn dyad_int_int(
    a: &JVal,
    w: &JVal,
    op: impl Fn(i64, i64) -> JResult<i64>,
) -> JResult<JVal> {
    match (a.as_int(), w.as_int()) {
        (Some(av), Some(wv)) => {
            let result = if av.len() == wv.len() {
                // element-wise
                av.iter().zip(wv.iter())
                    .map(|(&x, &y)| op(x, y))
                    .collect::<JResult<Vec<i64>>>()?
            } else if a.rank == 0 {
                // scalar a extends
                wv.iter().map(|&y| op(av[0], y))
                    .collect::<JResult<Vec<i64>>>()?
            } else if w.rank == 0 {
                // scalar w extends
                av.iter().map(|&x| op(x, wv[0]))
                    .collect::<JResult<Vec<i64>>>()?
            } else {
                return Err(length_err(format!(
                    "length mismatch: {} vs {}", av.len(), wv.len()
                )));
            };

            // shape 유지: a와 w 중 longer쪽의 shape 사용
            if result.len() == 1 {
                Ok(JArray::scalar_int(result[0]))
            } else if a.rank <= 1 && w.rank <= 1 {
                Ok(JArray::vector_int(result))
            } else {
                // 다차원: shape를 가진 쪽 기준
                let shape = if a.rank >= w.rank {
                    a.shape.clone()
                } else {
                    w.shape.clone()
                };
                Ok(JArray::array_int(shape, result))
            }
        }
        _ => Err(domain_err("integer arguments required")),
    }
}

// ─────────────────────────────────────────
// i. (iota)
// ─────────────────────────────────────────

pub struct Iota;

impl Verb for Iota {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        match w.as_int() {
            Some(v) => {
                if w.rank == 0 {
                    // i. n → 0 1 2 ... n-1
                    let n = v[0];
                    if n < 0 {
                        return Err(domain_err("i. requires non-negative integer"));
                    }
                    return Ok(JArray::vector_int((0..n).collect()));
                }
                if w.rank == 1 {
                    // i. 2 3 → 2x3 행렬
                    let shape: Vec<usize> = v.iter()
                        .map(|&x| if x < 0 { 0 } else { x as usize })
                        .collect();
                    let count: usize = shape.iter().product::<usize>().max(1);
                    let data: Vec<i64> = (0..count as i64).collect();
                    return Ok(JArray::array_int(shape, data));
                }
                Err(domain_err("i. requires scalar or vector argument"))
            }
            _ => Err(domain_err("i. requires integer argument")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> JResult<JVal> {
        Err(JError::no_loc(JErrorKind::Domain, "i. dyad not implemented"))
    }

    fn name(&self) -> &str { "i." }
}

// ─────────────────────────────────────────
// + (plus / conjugate)
// ─────────────────────────────────────────

pub struct Plus;

impl Verb for Plus {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad +: identity (정수의 경우)
        Ok(Arc::clone(w))
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        dyad_int_int(a, w, |x, y| Ok(x + y))
    }

    fn name(&self) -> &str { "+" }
}

// ─────────────────────────────────────────
// - (minus / negate)
// ─────────────────────────────────────────

pub struct Minus;

impl Verb for Minus {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad -: negate
        match w.as_int() {
            Some(v) => {
                let data: Vec<i64> = v.iter().map(|&x| -x).collect();
                if w.rank == 0 {
                    Ok(JArray::scalar_int(data[0]))
                } else if w.rank == 1 {
                    Ok(JArray::vector_int(data))
                } else {
                    Ok(JArray::array_int(w.shape.clone(), data))
                }
            }
            _ => Err(domain_err("- requires integer argument")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        dyad_int_int(a, w, |x, y| Ok(x - y))
    }

    fn name(&self) -> &str { "-" }
}

// ─────────────────────────────────────────
// * (times / signum)
// ─────────────────────────────────────────

pub struct Star;

impl Verb for Star {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad *: signum (-1, 0, 1)
        match w.as_int() {
            Some(v) => {
                let data: Vec<i64> = v.iter().map(|&x| x.signum()).collect();
                if w.rank == 0 {
                    Ok(JArray::scalar_int(data[0]))
                } else if w.rank == 1 {
                    Ok(JArray::vector_int(data))
                } else {
                    Ok(JArray::array_int(w.shape.clone(), data))
                }
            }
            _ => Err(domain_err("* requires integer argument")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        dyad_int_int(a, w, |x, y| Ok(x * y))
    }

    fn name(&self) -> &str { "*" }
}

// ─────────────────────────────────────────
// % (divide / reciprocal)
// ─────────────────────────────────────────

pub struct Percent;

impl Verb for Percent {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad %: reciprocal (정수면 0 또는 1)
        match w.as_int() {
            Some(v) => {
                let data: Vec<i64> = v.iter().map(|&x| {
                    if x == 0 { 0 } else { 1 / x }
                }).collect();
                if w.rank == 0 { Ok(JArray::scalar_int(data[0])) }
                else if w.rank == 1 { Ok(JArray::vector_int(data)) }
                else { Ok(JArray::array_int(w.shape.clone(), data)) }
            }
            _ => Err(domain_err("% requires integer argument")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        dyad_int_int(a, w, |x, y| {
            if y == 0 {
                Err(domain_err("divide by zero"))
            } else {
                Ok(x / y)
            }
        })
    }

    fn name(&self) -> &str { "%" }
}

// ─────────────────────────────────────────
// | (residue / magnitude)
// ─────────────────────────────────────────

pub struct Bar;

impl Verb for Bar {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad |: magnitude (absolute value)
        match w.as_int() {
            Some(v) => {
                let data: Vec<i64> = v.iter().map(|&x| x.abs()).collect();
                if w.rank == 0 { Ok(JArray::scalar_int(data[0])) }
                else if w.rank == 1 { Ok(JArray::vector_int(data)) }
                else { Ok(JArray::array_int(w.shape.clone(), data)) }
            }
            _ => Err(domain_err("| requires integer argument")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        // dyad |: residue (a | w = w mod a)
        // J에서 a | w 는 w를 a로 나눈 나머지
        dyad_int_int(a, w, |x, y| {
            if x == 0 {
                Ok(y)   // 0 | w = w (J 명세)
            } else {
                Ok(y.rem_euclid(x))  // 항상 양수 나머지
            }
        })
    }

    fn name(&self) -> &str { "|" }
}

// ─────────────────────────────────────────
// $ (shape / reshape)
// ─────────────────────────────────────────

pub struct Dollar;

impl Verb for Dollar {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad $: shape of w
        // $ (i. 2 3)  → 2 3
        // $ 5         → (empty, rank 0 has no shape)
        if w.rank == 0 {
            Ok(JArray::vector_int(vec![]))  // 스칼라의 shape = 빈 배열
        } else {
            let shape: Vec<i64> = w.shape.iter().map(|&x| x as i64).collect();
            Ok(JArray::vector_int(shape))
        }
    }

    fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        // dyad $: reshape
        // 2 3 $ i. 6  → 2x3 행렬
        match a.as_int() {
            Some(shape_v) => {
                let new_shape: Vec<usize> = shape_v.iter()
                    .map(|&x| {
                        if x < 0 { 0usize } else { x as usize }
                    })
                    .collect();
                let new_count: usize = new_shape.iter().product::<usize>().max(1);

                // w의 데이터를 순환하여 채움
                match w.as_int() {
                    Some(wv) => {
                        if wv.is_empty() {
                            return Err(domain_err("$ reshape: empty source"));
                        }
                        let data: Vec<i64> = (0..new_count)
                            .map(|i| wv[i % wv.len()])
                            .collect();
                        Ok(JArray::array_int(new_shape, data))
                    }
                    _ => Err(domain_err("$ reshape requires integer source")),
                }
            }
            _ => Err(domain_err("$ reshape requires integer shape")),
        }
    }

    fn name(&self) -> &str { "$" }
}

// ─────────────────────────────────────────
// # (tally / copy)
// ─────────────────────────────────────────

pub struct Hash;

impl Verb for Hash {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad #: leading axis 크기 (tally)
        Ok(JArray::scalar_int(w.tally() as i64))
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> JResult<JVal> {
        Err(JError::no_loc(JErrorKind::Domain, "# dyad not implemented"))
    }

    fn name(&self) -> &str { "#" }
}

// ─────────────────────────────────────────
// 비교 동사 (결과: 0 또는 1)
// ─────────────────────────────────────────

pub struct Lt;   // <
pub struct Gt;   // >
pub struct Le;   // <:
pub struct Ge;   // >:
pub struct Eq;   // =
pub struct Ne;   // ~:

macro_rules! impl_cmp_verb {
    ($t:ty, $sym:expr, $op:expr) => {
        impl Verb for $t {
            fn monad(&self, _interp: &Interpreter, _w: &JVal) -> JResult<JVal> {
                Err(JError::no_loc(JErrorKind::Domain,
                    concat!($sym, " monad not implemented")))
            }
            fn dyad(&self, _interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
                dyad_int_int(a, w, |x, y| Ok(if $op(x, y) { 1 } else { 0 }))
            }
            fn name(&self) -> &str { $sym }
        }
    };
}

impl_cmp_verb!(Lt, "<",  |x: i64, y: i64| x < y);
impl_cmp_verb!(Gt, ">",  |x: i64, y: i64| x > y);
impl_cmp_verb!(Le, "<:", |x: i64, y: i64| x <= y);
impl_cmp_verb!(Ge, ">:", |x: i64, y: i64| x >= y);
impl_cmp_verb!(Eq, "=",  |x: i64, y: i64| x == y);
impl_cmp_verb!(Ne, "~:", |x: i64, y: i64| x != y);



// ─────────────────────────────────────────
// / (slash) - adverb (fold)
// ─────────────────────────────────────────

pub struct Slash {
    pub u: VerbBox,
}

impl Verb for Slash {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        match w.as_int() {
            Some(v) => {
                if v.is_empty() {
                    return Err(domain_err("u/ requires non-empty array"));
                }
                // 오른쪽에서 왼쪽으로 fold
                let mut result = JArray::scalar_int(*v.last().unwrap());
                for &x in v.iter().rev().skip(1) {
                    let left = JArray::scalar_int(x);
                    result = self.u.dyad(interp, &left, &result)?;
                }
                Ok(result)
            }
            _ => Err(domain_err("u/ requires integer array")),
        }
    }

    fn dyad(&self, _interp: &Interpreter, _a: &JVal, _w: &JVal) -> JResult<JVal> {
        Err(JError::no_loc(JErrorKind::Domain, "/ dyad not implemented"))
    }

    fn name(&self) -> &str { "/" }
}

// ─────────────────────────────────────────
// Fork - tacit 합성
// (f g h) w = (f w) g (h w)
// ─────────────────────────────────────────

pub struct Fork {
    pub f: VerbBox,
    pub g: VerbBox,
    pub h: VerbBox,
}

impl Verb for Fork {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        let fw = self.f.monad(interp, w)?;
        let hw = self.h.monad(interp, w)?;
        self.g.dyad(interp, &fw, &hw)
    }

    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        let fw = self.f.dyad(interp, a, w)?;
        let hw = self.h.dyad(interp, a, w)?;
        self.g.dyad(interp, &fw, &hw)
    }

    fn name(&self) -> &str { "fork" }
}
