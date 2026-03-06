use crate::array::{JArray, JData, JVal, NumericType, VerbBox};
use crate::error::{JError, JErrorKind, JResult};
use crate::interp::Interpreter;
use std::sync::Arc;

// ─────────────────────────────────────────
// Verb trait
// ─────────────────────────────────────────

pub trait Verb: Send + Sync {
    fn monad_rank(&self) -> i64 { i64::MAX }
    fn dyad_rank(&self) -> (i64, i64) { (i64::MAX, i64::MAX) }

    /// 이 동사가 Complex를 지원하는가
    /// 비교 동사(<, >, =, ~:)는 false → domain error
    /// 산술 동사(+, -, *, %)는 true
    fn supports_complex(&self) -> bool { true }

    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal>;
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal>;
    fn name(&self) -> &str;
}

// ─────────────────────────────────────────
// 에러 헬퍼
// ─────────────────────────────────────────

fn domain_err(msg: impl Into<String>) -> JError {
    JError::no_loc(JErrorKind::Domain, msg)
}

fn length_err(msg: impl Into<String>) -> JError {
    JError::no_loc(JErrorKind::Length, msg)
}

// ─────────────────────────────────────────
// numeric tower 승격 헬퍼
// ─────────────────────────────────────────

/// 두 배열을 같은 타입으로 승격
/// Integer < Float < Complex
fn promote_pair(a: &JVal, w: &JVal) -> (JVal, JVal) {
    use NumericType::*;
    match (a.numeric_type(), w.numeric_type()) {
        (Some(Complex), _) | (_, Some(Complex)) =>
            (a.to_complex(), w.to_complex()),
        (Some(Float), _) | (_, Some(Float)) =>
            (a.to_float(), w.to_float()),
        _ => (Arc::clone(a), Arc::clone(w)),
    }
}

/// monad 적용 전 타입 확인
fn check_complex_monad(verb: &dyn Verb, w: &JVal) -> JResult<()> {
    if !verb.supports_complex() {
        if let Some(NumericType::Complex) = w.numeric_type() {
            return Err(domain_err(format!(
                "{} does not support complex arguments", verb.name()
            )));
        }
    }
    Ok(())
}

fn check_complex_dyad(verb: &dyn Verb, a: &JVal, w: &JVal) -> JResult<()> {
    if !verb.supports_complex() {
        let has_complex = matches!(a.numeric_type(), Some(NumericType::Complex))
            || matches!(w.numeric_type(), Some(NumericType::Complex));
        if has_complex {
            return Err(domain_err(format!(
                "{} does not support complex arguments", verb.name()
            )));
        }
    }
    Ok(())
}

// ─────────────────────────────────────────
// 공통 이항 연산 레이어
// scalar extension + shape 유지
// ─────────────────────────────────────────

/// 정수 이항 연산 - scalar extension 포함
fn dyad_int(a: &[i64], w: &[i64], a_rank: usize, w_rank: usize,
            op: impl Fn(i64, i64) -> JResult<i64>) -> JResult<Vec<i64>> {
    if a.len() == w.len() {
        a.iter().zip(w.iter()).map(|(&x, &y)| op(x, y)).collect()
    } else if a_rank == 0 {
        w.iter().map(|&y| op(a[0], y)).collect()
    } else if w_rank == 0 {
        a.iter().map(|&x| op(x, w[0])).collect()
    } else {
        Err(length_err(format!("length mismatch: {} vs {}", a.len(), w.len())))
    }
}

/// 실수 이항 연산 - scalar extension 포함
fn dyad_float(a: &[f64], w: &[f64], a_rank: usize, w_rank: usize,
              op: impl Fn(f64, f64) -> JResult<f64>) -> JResult<Vec<f64>> {
    if a.len() == w.len() {
        a.iter().zip(w.iter()).map(|(&x, &y)| op(x, y)).collect()
    } else if a_rank == 0 {
        w.iter().map(|&y| op(a[0], y)).collect()
    } else if w_rank == 0 {
        a.iter().map(|&x| op(x, w[0])).collect()
    } else {
        Err(length_err(format!("length mismatch: {} vs {}", a.len(), w.len())))
    }
}

/// 복소수 이항 연산 - scalar extension 포함
/// flat [r0,i0,r1,i1,...] 형식
fn dyad_complex(a: &[f64], w: &[f64], a_rank: usize, w_rank: usize,
                op: impl Fn((f64,f64),(f64,f64)) -> JResult<(f64,f64)>)
    -> JResult<Vec<f64>>
{
    let a_count = a.len() / 2;
    let w_count = w.len() / 2;

    let pairs: JResult<Vec<(f64,f64)>> = if a_count == w_count {
        a.chunks(2).zip(w.chunks(2))
            .map(|(ac, wc)| op((ac[0], ac[1]), (wc[0], wc[1])))
            .collect()
    } else if a_rank == 0 {
        w.chunks(2).map(|wc| op((a[0], a[1]), (wc[0], wc[1]))).collect()
    } else if w_rank == 0 {
        a.chunks(2).map(|ac| op((ac[0], ac[1]), (w[0], w[1]))).collect()
    } else {
        return Err(length_err(format!("length mismatch: {} vs {}", a_count, w_count)));
    };

    Ok(pairs?.into_iter().flat_map(|(r, i)| [r, i]).collect())
}

/// shape 보존하며 정수 JVal 생성
fn make_int(data: Vec<i64>, a: &JVal, w: &JVal) -> JVal {
    if data.len() == 1 {
        JArray::scalar_int(data[0])
    } else if a.rank <= 1 && w.rank <= 1 {
        JArray::vector_int(data)
    } else {
        let shape = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_int(shape, data)
    }
}

/// shape 보존하며 실수 JVal 생성
fn make_float(data: Vec<f64>, a: &JVal, w: &JVal) -> JVal {
    if data.len() == 1 {
        JArray::scalar_float(data[0])
    } else if a.rank <= 1 && w.rank <= 1 {
        JArray::vector_float(data)
    } else {
        let shape = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_float(shape, data)
    }
}

/// shape 보존하며 복소수 JVal 생성 (flat [r,i,...])
fn make_complex(flat: Vec<f64>, a: &JVal, w: &JVal) -> JVal {
    let count = flat.len() / 2;
    if count == 1 {
        JArray::scalar_complex(flat[0], flat[1])
    } else if a.rank <= 1 && w.rank <= 1 {
        JArray::array_complex(vec![count], flat)
    } else {
        let shape = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_complex(shape, flat)
    }
}

/// monad 용 shape 보존 생성 헬퍼
fn make_int_from(data: Vec<i64>, src: &JVal) -> JVal {
    if src.rank == 0 { JArray::scalar_int(data[0]) }
    else if src.rank == 1 { JArray::vector_int(data) }
    else { JArray::array_int(src.shape.clone(), data) }
}

fn make_float_from(data: Vec<f64>, src: &JVal) -> JVal {
    if src.rank == 0 { JArray::scalar_float(data[0]) }
    else if src.rank == 1 { JArray::vector_float(data) }
    else { JArray::array_float(src.shape.clone(), data) }
}

fn make_complex_from(flat: Vec<f64>, src: &JVal) -> JVal {
    let count = flat.len() / 2;
    if src.rank == 0 { JArray::scalar_complex(flat[0], flat[1]) }
    else if src.rank == 1 { JArray::array_complex(vec![count], flat) }
    else { JArray::array_complex(src.shape.clone(), flat) }
}

// ─────────────────────────────────────────
// 산술 동사를 위한 공통 매크로
// numeric tower를 통해 세 타입 모두 처리
// ─────────────────────────────────────────

/// dyad: Integer/Float/Complex 세 경로를 한 번에 처리
macro_rules! apply_dyad_numeric {
    ($a:expr, $w:expr,
     int:   $op_int:expr,
     float: $op_float:expr,
     cmpx:  $op_cmpx:expr
    ) => {{
        let (pa, pw) = promote_pair($a, $w);
        match (&pa.data, &pw.data) {
            (JData::Integer(av), JData::Integer(wv)) => {
                let r = dyad_int(av, wv, pa.rank, pw.rank, $op_int)?;
                Ok(make_int(r, &pa, &pw))
            }
            (JData::Float(av), JData::Float(wv)) => {
                let r = dyad_float(av, wv, pa.rank, pw.rank, $op_float)?;
                Ok(make_float(r, &pa, &pw))
            }
            (JData::Complex(av), JData::Complex(wv)) => {
                let r = dyad_complex(av, wv, pa.rank, pw.rank, $op_cmpx)?;
                Ok(make_complex(r, &pa, &pw))
            }
            _ => Err(domain_err("numeric arguments required")),
        }
    }};
}

/// monad: Integer/Float/Complex 세 경로를 한 번에 처리
macro_rules! apply_monad_numeric {
    ($w:expr,
     int:   $op_int:expr,
     float: $op_float:expr,
     cmpx:  $op_cmpx:expr
    ) => {{
        match &$w.data {
            JData::Integer(v) => {
                let r: JResult<Vec<i64>> = v.iter().map($op_int).collect();
                Ok(make_int_from(r?, $w))
            }
            JData::Float(v) => {
                let r: JResult<Vec<f64>> = v.iter().map($op_float).collect();
                Ok(make_float_from(r?, $w))
            }
            JData::Complex(v) => {
                // flat [r,i,...] → 쌍으로 처리
                let pairs: JResult<Vec<(f64,f64)>> = v.chunks(2)
                    .map(|c| $op_cmpx((c[0], c[1])))
                    .collect();
                let flat: Vec<f64> = pairs?.into_iter()
                    .flat_map(|(r,i)| [r,i]).collect();
                Ok(make_complex_from(flat, $w))
            }
            _ => Err(domain_err("numeric argument required")),
        }
    }};
}

// ─────────────────────────────────────────
// i. (iota) - Integer 전용
// ─────────────────────────────────────────

pub struct Iota;

impl Verb for Iota {
    fn monad(&self, _interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        match w.as_int() {
            Some(v) => {
                if w.rank == 0 {
                    let n = v[0];
                    if n < 0 { return Err(domain_err("i. requires non-negative integer")); }
                    return Ok(JArray::vector_int((0..n).collect()));
                }
                if w.rank == 1 {
                    let shape: Vec<usize> = v.iter()
                        .map(|&x| if x < 0 { 0 } else { x as usize })
                        .collect();
                    let count: usize = shape.iter().product::<usize>().max(1);
                    return Ok(JArray::array_int(shape, (0..count as i64).collect()));
                }
                Err(domain_err("i. requires scalar or vector argument"))
            }
            _ => Err(domain_err("i. requires integer argument")),
        }
    }
    fn dyad(&self, _: &Interpreter, _: &JVal, _: &JVal) -> JResult<JVal> {
        Err(domain_err("i. dyad not implemented"))
    }
    fn name(&self) -> &str { "i." }
}

// ─────────────────────────────────────────
// + (plus / conjugate)
// monad +: identity(int/float), conjugate(complex)
// ─────────────────────────────────────────

pub struct Plus;

impl Verb for Plus {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self) -> (i64, i64) { (0, 0) }

    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(x),                      // identity
            float: |&x| Ok(x),                      // identity
            cmpx:  |(r, i)| Ok((r, -i))             // conjugate
        )
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(a, w,
            int:   |x, y| Ok(x + y),
            float: |x, y| Ok(x + y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar+wr, ai+wi))
        )
    }
    fn name(&self) -> &str { "+" }
}

// ─────────────────────────────────────────
// - (minus / negate)
// ─────────────────────────────────────────

pub struct Minus;

impl Verb for Minus {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self) -> (i64, i64) { (0, 0) }

    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(-x),
            float: |&x| Ok(-x),
            cmpx:  |(r,i)| Ok((-r, -i))
        )
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(a, w,
            int:   |x, y| Ok(x - y),
            float: |x, y| Ok(x - y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar-wr, ai-wi))
        )
    }
    fn name(&self) -> &str { "-" }
}

// ─────────────────────────────────────────
// * (times / signum)
// monad *: signum(int/float), unit complex(cmpx)
// ─────────────────────────────────────────

pub struct Star;

impl Verb for Star {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self) -> (i64, i64) { (0, 0) }

    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(x.signum()),
            float: |&x: &f64| Ok(if *x == 0.0 { 0.0 } else { x.signum() }),
            cmpx:  |(r,i): (f64,f64)| {
                // unit complex: w / |w|
                let mag = (r*r + i*i).sqrt();
                if mag == 0.0 { Ok((0.0, 0.0)) }
                else { Ok((r/mag, i/mag)) }
            }
        )
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(a, w,
            int:   |x, y| Ok(x * y),
            float: |x, y| Ok(x * y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar*wr - ai*wi, ar*wi + ai*wr))
        )
    }
    fn name(&self) -> &str { "*" }
}

// ─────────────────────────────────────────
// % (divide / reciprocal)
// ─────────────────────────────────────────

pub struct Percent;

impl Verb for Percent {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self) -> (i64, i64) { (0, 0) }

    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        // monad %: reciprocal → 실수로 승격
        let fw = w.to_float();
        apply_monad_numeric!(&fw,
            int:   |_| Err(domain_err("unreachable")),
            float: |&x| if x == 0.0 {
                Err(domain_err("% monad: divide by zero"))
            } else { Ok(1.0 / x) },
            cmpx:  |(r,i): (f64,f64)| {
                let d = r*r + i*i;
                if d == 0.0 { Err(domain_err("% monad: divide by zero")) }
                else { Ok((r/d, -i/d)) }
            }
        )
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        // dyad %: 실수로 승격 후 나눗셈
        let (pa, pw) = (a.to_float(), w.to_float());
        apply_dyad_numeric!(&pa, &pw,
            int:   |_, _| Err(domain_err("unreachable")),
            float: |x, y| if y == 0.0 {
                Err(domain_err("% dyad: divide by zero"))
            } else { Ok(x / y) },
            cmpx:  |(ar,ai),(wr,wi)| {
                let d = wr*wr + wi*wi;
                if d == 0.0 { Err(domain_err("% dyad: divide by zero")) }
                else { Ok(((ar*wr + ai*wi)/d, (ai*wr - ar*wi)/d)) }
            }
        )
    }
    fn name(&self) -> &str { "%" }
}

// ─────────────────────────────────────────
// | (residue / magnitude)
// Complex: magnitude (실수 반환)
// ─────────────────────────────────────────

pub struct Bar;

impl Verb for Bar {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self) -> (i64, i64) { (0, 0) }

    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        match &w.data {
            JData::Integer(v) => {
                let r: Vec<i64> = v.iter().map(|&x| x.abs()).collect();
                Ok(make_int_from(r, w))
            }
            JData::Float(v) => {
                let r: Vec<f64> = v.iter().map(|x| x.abs()).collect();
                Ok(make_float_from(r, w))
            }
            JData::Complex(v) => {
                // complex magnitude → float
                let r: Vec<f64> = v.chunks(2)
                    .map(|c| (c[0]*c[0] + c[1]*c[1]).sqrt())
                    .collect();
                Ok(make_float_from(r, w))
            }
            _ => Err(domain_err("| requires numeric argument")),
        }
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        // dyad |: residue - complex는 domain error
        check_complex_dyad(self, a, w)?;
        apply_dyad_numeric!(a, w,
            int:   |x, y| Ok(if x == 0 { y } else { y.rem_euclid(x) }),
            float: |x: f64, y: f64| Ok(if x == 0.0 { y } else { y.rem_euclid(x) }),
            cmpx:  |_, _| Err(domain_err("| residue not defined for complex"))
        )
    }
    fn supports_complex(&self) -> bool { false }  // dyad | 는 complex 불가
    fn name(&self) -> &str { "|" }
}

// ─────────────────────────────────────────
// $ (shape / reshape)
// ─────────────────────────────────────────

pub struct Dollar;

impl Verb for Dollar {
    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        if w.rank == 0 { return Ok(JArray::vector_int(vec![])); }
        Ok(JArray::vector_int(w.shape.iter().map(|&x| x as i64).collect()))
    }

    fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        let shape_v = a.as_int().ok_or_else(|| domain_err("$ requires integer shape"))?;
        let new_shape: Vec<usize> = shape_v.iter()
            .map(|&x| if x < 0 { 0usize } else { x as usize })
            .collect();
        let new_count: usize = new_shape.iter().product::<usize>().max(1);

        match &w.data {
            JData::Integer(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                let data: Vec<i64> = (0..new_count).map(|i| wv[i % wv.len()]).collect();
                Ok(JArray::array_int(new_shape, data))
            }
            JData::Float(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                let data: Vec<f64> = (0..new_count).map(|i| wv[i % wv.len()]).collect();
                Ok(JArray::array_float(new_shape, data))
            }
            JData::Complex(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                let pair_count = wv.len() / 2;
                let flat: Vec<f64> = (0..new_count)
                    .flat_map(|i| { let p = i % pair_count; [wv[p*2], wv[p*2+1]] })
                    .collect();
                Ok(JArray::array_complex(new_shape, flat))
            }
            _ => Err(domain_err("$ requires numeric source")),
        }
    }
    fn name(&self) -> &str { "$" }
}

// ─────────────────────────────────────────
// # (tally)
// ─────────────────────────────────────────

pub struct Hash;

impl Verb for Hash {
    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        Ok(JArray::scalar_int(w.tally() as i64))
    }
    fn dyad(&self, _: &Interpreter, _: &JVal, _: &JVal) -> JResult<JVal> {
        Err(domain_err("# dyad not implemented"))
    }
    fn name(&self) -> &str { "#" }
}

// ─────────────────────────────────────────
// 비교 동사 - Complex 불가
// ─────────────────────────────────────────

pub struct Lt;  pub struct Gt;
pub struct Le;  pub struct Ge;
pub struct Eq;  pub struct Ne;

macro_rules! impl_cmp_verb {
    ($t:ty, $sym:expr, $op_int:expr, $op_float:expr) => {
        impl Verb for $t {
            fn monad_rank(&self) -> i64 { 0 }
            fn dyad_rank(&self) -> (i64, i64) { (0, 0) }
            fn supports_complex(&self) -> bool { false }

            fn monad(&self, _: &Interpreter, _: &JVal) -> JResult<JVal> {
                Err(domain_err(concat!($sym, " monad not implemented")))
            }

            fn dyad(&self, _: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
                check_complex_dyad(self, a, w)?;
                apply_dyad_numeric!(a, w,
                    int:   |x, y| Ok(if $op_int(x, y) { 1i64 } else { 0i64 }),
                    float: |x, y| Ok(if $op_float(x, y) { 1i64 } else { 0i64 }),
                    cmpx:  |_, _| Err(domain_err(concat!($sym, " not defined for complex")))
                ).map(|v| {
                    // 결과를 Integer로 변환 (0 또는 1)
                    match &v.data {
                        JData::Float(fv) => {
                            let iv: Vec<i64> = fv.iter().map(|&x| x as i64).collect();
                            make_int(iv, a, w)
                        }
                        _ => v
                    }
                })
            }
            fn name(&self) -> &str { $sym }
        }
    };
}

impl_cmp_verb!(Lt, "<",  |x:i64,y:i64| x<y,  |x:f64,y:f64| x<y);
impl_cmp_verb!(Gt, ">",  |x:i64,y:i64| x>y,  |x:f64,y:f64| x>y);
impl_cmp_verb!(Le, "<:", |x:i64,y:i64| x<=y, |x:f64,y:f64| x<=y);
impl_cmp_verb!(Ge, ">:", |x:i64,y:i64| x>=y, |x:f64,y:f64| x>=y);
impl_cmp_verb!(Eq, "=",  |x:i64,y:i64| x==y, |x:f64,y:f64| x==y);
impl_cmp_verb!(Ne, "~:", |x:i64,y:i64| x!=y, |x:f64,y:f64| x!=y);

// ─────────────────────────────────────────
// / (slash) - fold
// ─────────────────────────────────────────

pub struct Slash { pub u: VerbBox }

impl Verb for Slash {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        let tally = w.tally();
        if tally == 0 {
            return Err(domain_err("u/ requires non-empty array"));
        }

        // 오른쪽에서 왼쪽으로 fold
        // 마지막 원소부터 시작
        let cell_size = w.count / tally;
        let mut result = extract_cell_generic(w, tally - 1, cell_size);
        for i in (0..tally - 1).rev() {
            let left = extract_cell_generic(w, i, cell_size);
            result = self.u.dyad(interp, &left, &result)?;
        }
        Ok(result)
    }

    fn dyad(&self, _: &Interpreter, _: &JVal, _: &JVal) -> JResult<JVal> {
        Err(domain_err("/ dyad not implemented"))
    }
    fn name(&self) -> &str { "/" }
}

/// 배열에서 i번째 cell 추출 (타입 무관)
fn extract_cell_generic(w: &JVal, i: usize, cell_size: usize) -> JVal {
    let start = i * cell_size;
    let end   = start + cell_size;
    let shape = if w.rank <= 1 { vec![] }
                else { w.shape[1..].to_vec() };

    match &w.data {
        JData::Integer(v) => {
            let s = v[start..end].to_vec();
            if shape.is_empty() { JArray::scalar_int(s[0]) }
            else if shape.len() == 1 { JArray::vector_int(s) }
            else { JArray::array_int(shape, s) }
        }
        JData::Float(v) => {
            let s = v[start..end].to_vec();
            if shape.is_empty() { JArray::scalar_float(s[0]) }
            else if shape.len() == 1 { JArray::vector_float(s) }
            else { JArray::array_float(shape, s) }
        }
        JData::Complex(v) => {
            let cs = start * 2;
            let ce = end * 2;
            let s = v[cs..ce].to_vec();
            if shape.is_empty() { JArray::scalar_complex(s[0], s[1]) }
            else if shape.len() == 1 { JArray::array_complex(vec![cell_size], s) }
            else { JArray::array_complex(shape, s) }
        }
        _ => panic!("extract_cell_generic: non-numeric type"),
    }
}

// ─────────────────────────────────────────
// Fork - tacit 합성
// ─────────────────────────────────────────

pub struct Fork { pub f: VerbBox, pub g: VerbBox, pub h: VerbBox }

impl Verb for Fork {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
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
// ─────────────────────────────────────────

pub fn rank1ex(verb: &dyn Verb, interp: &Interpreter, w: &JVal, verb_rank: i64) -> JResult<JVal> {
    if verb_rank == i64::MAX || w.rank as i64 <= verb_rank {
        return verb.monad(interp, w);
    }

    let leading   = w.tally();
    let cell_rank = verb_rank.max(0) as usize;
    let cell_size = w.count / leading;

    let mut results: Vec<JVal> = Vec::with_capacity(leading);
    for i in 0..leading {
        let cell = extract_cell_generic(w, i, cell_size);
        results.push(verb.monad(interp, &cell)?);
    }

    let frame = &w.shape[..w.rank - cell_rank.min(w.rank)];
    assemble_results(results, leading, frame)
}

pub fn rank2ex(verb: &dyn Verb, interp: &Interpreter, a: &JVal, w: &JVal,
               lrank: i64, rrank: i64) -> JResult<JVal>
{
    let a_ok = lrank == i64::MAX || a.rank as i64 <= lrank;
    let w_ok = rrank == i64::MAX || w.rank as i64 <= rrank;
    if a_ok && w_ok {
        return verb.dyad(interp, a, w);
    }

    let eff_lrank = effective_rank(a.rank, lrank);
    let eff_rrank = effective_rank(w.rank, rrank);

    let a_leading = if eff_lrank >= a.rank { 1 } else { a.shape[0] };
    let w_leading = if eff_rrank >= w.rank { 1 } else { w.shape[0] };

    if a_leading != w_leading && a_leading != 1 && w_leading != 1 {
        return Err(JError::no_loc(JErrorKind::Length,
            format!("rank agreement: frame mismatch {} vs {}", a_leading, w_leading)));
    }

    let leading      = a_leading.max(w_leading);
    let a_cell_size  = a.count / a_leading;
    let w_cell_size  = w.count / w_leading;

    let mut results: Vec<JVal> = Vec::with_capacity(leading);
    for i in 0..leading {
        let ai = if a_leading == 1 { 0 } else { i };
        let wi = if w_leading == 1 { 0 } else { i };
        let a_cell = extract_cell_generic(a, ai, a_cell_size);
        let w_cell = extract_cell_generic(w, wi, w_cell_size);
        results.push(verb.dyad(interp, &a_cell, &w_cell)?);
    }

    let frame = if a.rank > eff_lrank { &a.shape[..a.rank - eff_lrank] }
                else if w.rank > eff_rrank { &w.shape[..w.rank - eff_rrank] }
                else { &[] };

    assemble_results(results, leading, frame)
}

fn effective_rank(arg_rank: usize, verb_rank: i64) -> usize {
    if verb_rank == i64::MAX { arg_rank }
    else if verb_rank >= 0 { (verb_rank as usize).min(arg_rank) }
    else { (arg_rank as i64 + verb_rank).max(0) as usize }
}

fn assemble_results(results: Vec<JVal>, leading: usize, frame: &[usize]) -> JResult<JVal> {
    if results.is_empty() {
        return Ok(JArray::vector_int(vec![]));
    }
    if leading == 1 && frame.is_empty() {
        return Ok(Arc::clone(&results[0]));
    }

    let result_shape = &results[0].shape;
    let mut full_shape: Vec<usize> = frame.to_vec();
    full_shape.extend_from_slice(result_shape);

    match &results[0].data {
        JData::Integer(_) => {
            let mut flat: Vec<i64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_int()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            if full_shape.is_empty() { Ok(JArray::scalar_int(flat[0])) }
            else if full_shape.len() == 1 { Ok(JArray::vector_int(flat)) }
            else { Ok(JArray::array_int(full_shape, flat)) }
        }
        JData::Float(_) => {
            let mut flat: Vec<f64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_float()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            if full_shape.is_empty() { Ok(JArray::scalar_float(flat[0])) }
            else if full_shape.len() == 1 { Ok(JArray::vector_float(flat)) }
            else { Ok(JArray::array_float(full_shape, flat)) }
        }
        JData::Complex(_) => {
            let mut flat: Vec<f64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_complex()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            let count = flat.len() / 2;
            if full_shape.is_empty() { Ok(JArray::scalar_complex(flat[0], flat[1])) }
            else if full_shape.len() == 1 { Ok(JArray::array_complex(vec![count], flat)) }
            else { Ok(JArray::array_complex(full_shape, flat)) }
        }
        _ => Err(domain_err("assemble: non-numeric result")),
    }
}
