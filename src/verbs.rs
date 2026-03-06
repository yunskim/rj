use crate::array::{JArray, JVal, VerbBox};
use crate::error::{JError, JErrorKind, JResult};
use crate::interp::Interpreter;
use std::sync::Arc;

/// J의 모든 동사가 구현하는 trait
/// Send + Sync: 멀티스레드 안전
pub trait Verb: Send + Sync {
    /// monad 적용 시 요구하는 rank
    /// J의 VRANK 필드 중 monad rank
    /// i64::MAX = 배열 전체 (무한 rank)
    fn monad_rank(&self) -> i64 { i64::MAX }

    /// dyad 적용 시 요구하는 rank (좌, 우)
    /// J의 VRANK 필드 중 lrank, rrank
    fn dyad_rank(&self) -> (i64, i64) { (i64::MAX, i64::MAX) }

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
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
            fn monad_rank(&self) -> i64 { 0 }
            fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
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
        // f, h 각각 rank agreement 적용 후 g로 결합
        let fw = rank1ex(self.f.as_ref(), interp, w, self.f.monad_rank())?;
        let hw = rank1ex(self.h.as_ref(), interp, w, self.h.monad_rank())?;
        rank2ex(self.g.as_ref(), interp, &fw, &hw,
                self.g.dyad_rank().0, self.g.dyad_rank().1)
    }

    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        let fw = rank2ex(self.f.as_ref(), interp, a, w,
                         self.f.dyad_rank().0, self.f.dyad_rank().1)?;
        let hw = rank2ex(self.h.as_ref(), interp, a, w,
                         self.h.dyad_rank().0, self.h.dyad_rank().1)?;
        rank2ex(self.g.as_ref(), interp, &fw, &hw,
                self.g.dyad_rank().0, self.g.dyad_rank().1)
    }

    fn name(&self) -> &str { "fork" }
}

// ─────────────────────────────────────────
// rank1ex / rank2ex
// J의 DF1RANK / DF2RANK 에 해당
//
// 동사 실행 전 레이어:
//   배열의 rank > 동사의 rank 이면
//   → leading axis 따라 cell 분해
//   → 각 cell에 동사 적용
//   → 결과를 다시 조립
// ─────────────────────────────────────────

/// J의 rank1ex 에 해당
/// monad 적용 시 rank agreement 처리
///
/// verb_rank: 동사가 요구하는 rank (i64::MAX = 전체 배열)
/// w의 rank > verb_rank 이면 leading axis로 cell 분해
///
/// 예: verb_rank=0, w=[[1,2],[3,4]] (rank 2)
///     → cell[0]=[1,2] 에 적용, cell[1]=[3,4] 에 적용
///     → 결과 조립
pub fn rank1ex(
    verb:      &dyn Verb,
    interp:    &Interpreter,
    w:         &JVal,
    verb_rank: i64,
) -> JResult<JVal> {
    // AR(w) <= verb_rank 이면 직접 실행 (J의 DF1RANK 첫 번째 분기)
    if verb_rank == i64::MAX || w.rank as i64 <= verb_rank {
        return verb.monad(interp, w);
    }

    // cell 크기 계산
    // verb_rank=0, w.rank=2, w.shape=[2,3]
    // → leading axis = 2 (shape[0])
    // → cell shape = [3] (나머지)
    let leading   = w.tally();
    let cell_rank = verb_rank.max(0) as usize;
    let cell_shape: Vec<usize> = w.shape[w.rank - cell_rank..].to_vec();
    let cell_size: usize = cell_shape.iter().product::<usize>().max(1);

    // 각 cell에 동사 적용
    let mut results: Vec<JVal> = Vec::with_capacity(leading);
    match w.as_int() {
        Some(data) => {
            for i in 0..leading {
                let slice = data[i * cell_size..(i + 1) * cell_size].to_vec();
                let cell = if cell_shape.is_empty() {
                    JArray::scalar_int(slice[0])
                } else {
                    JArray::array_int(cell_shape.clone(), slice)
                };
                results.push(verb.monad(interp, &cell)?);
            }
        }
        _ => return Err(JError::no_loc(JErrorKind::Domain, "rank1ex: integer array required")),
    }

    assemble_results(results, leading, &w.shape[..w.rank - cell_rank])
}

/// J의 rank2ex 에 해당
/// dyad 적용 시 rank agreement 처리
///
/// lrank: 왼쪽 인자(a)에 요구하는 rank
/// rrank: 오른쪽 인자(w)에 요구하는 rank
pub fn rank2ex(
    verb:   &dyn Verb,
    interp: &Interpreter,
    a:      &JVal,
    w:      &JVal,
    lrank:  i64,
    rrank:  i64,
) -> JResult<JVal> {
    // 양쪽 모두 rank 조건 만족 → 직접 실행 (J의 DF2RANK 첫 번째 분기)
    let a_ok = lrank == i64::MAX || a.rank as i64 <= lrank;
    let w_ok = rrank == i64::MAX || w.rank as i64 <= rrank;
    if a_ok && w_ok {
        return verb.dyad(interp, a, w);
    }

    // effective rank 계산 (J의 efr 매크로)
    // efr(z, ar, r): ar=인자 rank, r=동사 rank
    // z = min(ar, r) 단 r<0 이면 ar+r
    let eff_lrank = effective_rank(a.rank, lrank);
    let eff_rrank = effective_rank(w.rank, rrank);

    let a_leading = if eff_lrank >= a.rank { 1 } else { a.shape[0] };
    let w_leading = if eff_rrank >= w.rank { 1 } else { w.shape[0] };

    // frame 검사: leading axis가 맞아야 함
    // scalar(1) 는 항상 확장 가능
    if a_leading != w_leading && a_leading != 1 && w_leading != 1 {
        return Err(JError::no_loc(JErrorKind::Length,
            format!("rank agreement: frame mismatch {} vs {}", a_leading, w_leading)));
    }

    let leading = a_leading.max(w_leading);

    let a_cell_shape: Vec<usize> = a.shape[a.rank.saturating_sub(eff_lrank)..].to_vec();
    let w_cell_shape: Vec<usize> = w.shape[w.rank.saturating_sub(eff_rrank)..].to_vec();
    let a_cell_size: usize = a_cell_shape.iter().product::<usize>().max(1);
    let w_cell_size: usize = w_cell_shape.iter().product::<usize>().max(1);

    let mut results: Vec<JVal> = Vec::with_capacity(leading);

    match (a.as_int(), w.as_int()) {
        (Some(ad), Some(wd)) => {
            for i in 0..leading {
                let ai = if a_leading == 1 { 0 } else { i };
                let wi = if w_leading == 1 { 0 } else { i };

                let a_slice = ad[ai * a_cell_size..(ai + 1) * a_cell_size].to_vec();
                let w_slice = wd[wi * w_cell_size..(wi + 1) * w_cell_size].to_vec();

                let a_cell = make_cell(a_cell_shape.clone(), a_slice);
                let w_cell = make_cell(w_cell_shape.clone(), w_slice);

                results.push(verb.dyad(interp, &a_cell, &w_cell)?);
            }
        }
        _ => return Err(JError::no_loc(JErrorKind::Domain, "rank2ex: integer arrays required")),
    }

    // frame: a와 w의 leading axis 부분
    let frame = if a.rank > eff_lrank {
        &a.shape[..a.rank - eff_lrank]
    } else if w.rank > eff_rrank {
        &w.shape[..w.rank - eff_rrank]
    } else {
        &[]
    };

    assemble_results(results, leading, frame)
}

/// J의 efr 매크로에 해당
/// effective rank: 인자 rank와 동사 rank로 실제 cell rank 계산
fn effective_rank(arg_rank: usize, verb_rank: i64) -> usize {
    if verb_rank == i64::MAX {
        arg_rank
    } else if verb_rank >= 0 {
        (verb_rank as usize).min(arg_rank)
    } else {
        // 음수 rank: arg_rank + verb_rank (뒤에서부터)
        let r = arg_rank as i64 + verb_rank;
        r.max(0) as usize
    }
}

/// cell shape로 JVal 생성
fn make_cell(shape: Vec<usize>, data: Vec<i64>) -> JVal {
    if shape.is_empty() {
        JArray::scalar_int(data[0])
    } else if shape.len() == 1 {
        JArray::vector_int(data)
    } else {
        JArray::array_int(shape, data)
    }
}

/// 각 cell의 결과를 조립해서 최종 배열 생성
/// frame + result_shape 로 전체 shape 결정
fn assemble_results(results: Vec<JVal>, leading: usize, frame: &[usize]) -> JResult<JVal> {
    if results.is_empty() {
        return Ok(JArray::vector_int(vec![]));
    }

    // 모든 결과의 shape이 같다고 가정 (J의 동작과 동일)
    let result_shape = &results[0].shape;
    let result_rank  = results[0].rank;

    // 전체 shape = frame + result_shape
    let mut full_shape: Vec<usize> = frame.to_vec();
    full_shape.extend_from_slice(result_shape);

    // 결과가 스칼라들이고 leading=1 이면 그냥 반환
    if leading == 1 && frame.is_empty() {
        return Ok(Arc::clone(&results[0]));
    }

    // flat data 조립
    let mut flat: Vec<i64> = Vec::new();
    for r in &results {
        match r.as_int() {
            Some(v) => flat.extend_from_slice(v),
            None    => return Err(JError::no_loc(JErrorKind::Domain, "assemble: non-integer result")),
        }
    }

    if full_shape.is_empty() {
        Ok(JArray::scalar_int(flat[0]))
    } else if full_shape.len() == 1 {
        Ok(JArray::vector_int(flat))
    } else {
        Ok(JArray::array_int(full_shape, flat))
    }
}
