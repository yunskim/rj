use crate::array::{JArray, JData, JVal, NumericType, VerbBox};
use crate::error::{JError, JErrorKind, JResult};
use crate::gpu::Backend;
use crate::interp::Interpreter;
use std::sync::Arc;

// ─────────────────────────────────────────
// Verb trait
// ─────────────────────────────────────────

pub trait Verb: Send + Sync {
    fn monad_rank(&self) -> i64 { i64::MAX }
    fn dyad_rank(&self) -> (i64, i64) { (i64::MAX, i64::MAX) }

    /// Complex 지원 여부
    /// 비교 동사(<, >, =, ~:)는 false
    fn supports_complex(&self) -> bool { true }

    /// GPU/태스크 실행 지원 여부
    /// 기본값: false - 모든 동사는 기본적으로 CPU
    /// t. conjunction 으로 감싸야만 true
    /// 사용자가 명시적으로 t. 를 붙인 동사만 GPU로 실행
    fn supports_gpu(&self) -> bool { false }

    /// GPU op_code 반환
    /// float_binop / complex_binop 셰이더에 전달
    /// 0=add, 1=sub, 2=mul, 3=div
    /// Tco 가 내부 동사에서 이 값을 꺼내 커널에 전달
    fn gpu_dyad_op_code(&self) -> Option<u32> { None }

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
// numeric tower 승격
// ─────────────────────────────────────────

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

fn check_complex_dyad(verb: &dyn Verb, a: &JVal, w: &JVal) -> JResult<()> {
    if !verb.supports_complex() {
        let has = matches!(a.numeric_type(), Some(NumericType::Complex))
               || matches!(w.numeric_type(), Some(NumericType::Complex));
        if has {
            return Err(domain_err(format!(
                "{} does not support complex arguments", verb.name()
            )));
        }
    }
    Ok(())
}

// ─────────────────────────────────────────
// CPU 이항 연산 헬퍼 (scalar extension 포함)
// ─────────────────────────────────────────

fn dyad_int(a: &[i64], w: &[i64], ar: usize, wr: usize,
            op: impl Fn(i64, i64) -> JResult<i64>) -> JResult<Vec<i64>> {
    if a.len() == w.len() {
        a.iter().zip(w).map(|(&x,&y)| op(x,y)).collect()
    } else if ar == 0 {
        w.iter().map(|&y| op(a[0],y)).collect()
    } else if wr == 0 {
        a.iter().map(|&x| op(x,w[0])).collect()
    } else {
        Err(length_err(format!("length mismatch: {} vs {}", a.len(), w.len())))
    }
}

fn dyad_float(a: &[f64], w: &[f64], ar: usize, wr: usize,
              op: impl Fn(f64, f64) -> JResult<f64>) -> JResult<Vec<f64>> {
    if a.len() == w.len() {
        a.iter().zip(w).map(|(&x,&y)| op(x,y)).collect()
    } else if ar == 0 {
        w.iter().map(|&y| op(a[0],y)).collect()
    } else if wr == 0 {
        a.iter().map(|&x| op(x,w[0])).collect()
    } else {
        Err(length_err(format!("length mismatch: {} vs {}", a.len(), w.len())))
    }
}

fn dyad_complex(a: &[f64], w: &[f64], ar: usize, wr: usize,
                op: impl Fn((f64,f64),(f64,f64)) -> JResult<(f64,f64)>)
    -> JResult<Vec<f64>>
{
    let ac = a.len() / 2;
    let wc = w.len() / 2;
    let pairs: JResult<Vec<(f64,f64)>> = if ac == wc {
        a.chunks(2).zip(w.chunks(2))
            .map(|(ac,wc)| op((ac[0],ac[1]),(wc[0],wc[1]))).collect()
    } else if ar == 0 {
        w.chunks(2).map(|wc| op((a[0],a[1]),(wc[0],wc[1]))).collect()
    } else if wr == 0 {
        a.chunks(2).map(|ac| op((ac[0],ac[1]),(w[0],w[1]))).collect()
    } else {
        return Err(length_err(format!("length mismatch: {} vs {}", ac, wc)));
    };
    Ok(pairs?.into_iter().flat_map(|(r,i)| [r,i]).collect())
}

// ─────────────────────────────────────────
// shape 보존 생성 헬퍼
// ─────────────────────────────────────────

fn make_int(data: Vec<i64>, a: &JVal, w: &JVal) -> JVal {
    if data.len() == 1 { JArray::scalar_int(data[0]) }
    else if a.rank <= 1 && w.rank <= 1 { JArray::vector_int(data) }
    else {
        let s = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_int(s, data)
    }
}

fn make_float(data: Vec<f64>, a: &JVal, w: &JVal) -> JVal {
    if data.len() == 1 { JArray::scalar_float(data[0]) }
    else if a.rank <= 1 && w.rank <= 1 { JArray::vector_float(data) }
    else {
        let s = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_float(s, data)
    }
}

fn make_complex(flat: Vec<f64>, a: &JVal, w: &JVal) -> JVal {
    let count = flat.len() / 2;
    if count == 1 { JArray::scalar_complex(flat[0], flat[1]) }
    else if a.rank <= 1 && w.rank <= 1 { JArray::array_complex(vec![count], flat) }
    else {
        let s = if a.rank >= w.rank { a.shape.clone() } else { w.shape.clone() };
        JArray::array_complex(s, flat)
    }
}

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
// apply_dyad_numeric / apply_monad_numeric
//
// CPU 경로: numeric tower 승격 후 타입별 분기
// GPU 경로: 데이터를 VRAM으로 올리고 커널 호출
//
// GPU 경로의 핵심:
//   a, w 가 이미 GpuFloat이면 전송 없이 그대로 사용
//   결과도 GpuFloat으로 반환 → 중간 결과가 VRAM에 머묾
//   출력(Display)할 때만 to_cpu() 호출
// ─────────────────────────────────────────

macro_rules! apply_dyad_numeric {
    ($interp:expr, $a:expr, $w:expr,
     int:   $op_int:expr,
     float: $op_float:expr,
     cmpx:  $op_cmpx:expr
    ) => {{
        // GPU 경로
        #[cfg(feature = "gpu")]
        if let Backend::Gpu(dev) = &$interp.backend {
            // GPU 지원 여부는 호출 전에 확인됨 (rank1ex_gpu에서)
            // 여기서는 데이터 전송만 처리
            let ag = $a.to_gpu(dev);
            let wg = $w.to_gpu(dev);
            // 실제 GPU 커널 호출은 gpu_dispatch_dyad 에서
            return gpu_dispatch_dyad(dev, &ag, &wg);
        }

        // CPU 경로
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
                let pairs: JResult<Vec<(f64,f64)>> = v.chunks(2)
                    .map(|c| $op_cmpx((c[0], c[1]))).collect();
                let flat: Vec<f64> = pairs?.into_iter()
                    .flat_map(|(r,i)| [r,i]).collect();
                Ok(make_complex_from(flat, $w))
            }
            // GPU 데이터에 monad를 적용하는 경우
            // 지금은 CPU로 가져와서 처리 (추후 GPU monad 커널 추가 가능)
            #[cfg(feature = "gpu")]
            JData::GpuFloat(_) | JData::GpuComplex(_) => {
                Err(domain_err("GPU monad path not yet implemented"))
            }
            _ => Err(domain_err("numeric argument required")),
        }
    }};
}

// ─────────────────────────────────────────
// GPU dyad 디스패치
// 커널 선택 → 바인딩 → 실행
// 결과는 VRAM에 그대로 (GpuFloat 반환)
// ─────────────────────────────────────────

#[cfg(feature = "gpu")]
fn gpu_dispatch_dyad(
    dev: &crate::gpu::GpuDevice,
    a:   &JVal,
    w:   &JVal,
) -> JResult<JVal> {
    use crate::gpu::GpuBuffer;

    // 이 함수에 오는 시점에서 a, w 는 이미 GpuFloat 또는 GpuComplex
    let (a_buf, w_buf, is_complex) = match (&a.data, &w.data) {
        (JData::GpuFloat(ab), JData::GpuFloat(wb)) =>
            (ab.clone(), wb.clone(), false),
        (JData::GpuComplex(ab), JData::GpuComplex(wb)) =>
            (ab.clone(), wb.clone(), true),
        _ => return Err(domain_err("gpu_dispatch_dyad: mismatched types")),
    };

    // 결과 버퍼 생성 (VRAM, 비어있음)
    let result_size = if is_complex {
        (a.count * 2 * std::mem::size_of::<f64>()) as u64
    } else {
        (a.count * std::mem::size_of::<f64>()) as u64
    };

    let result_buf = dev.device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("result"),
        size:               result_size,
        usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // bind group, dispatch 생략 (실제 구현에서는 커널별 bind group layout 필요)
    // 여기서는 구조만 표현
    // ...

    // 결과 JVal 반환 (VRAM에 있는 GpuFloat/GpuComplex)
    let result_gpu_buf = if is_complex {
        let gb = GpuBuffer {
            count:     a.count,
            elem_type: crate::gpu::GpuElemType::Complex,
            inner: crate::gpu::GpuBufferInner {
                buffer: result_buf,
                size:   result_size,
            },
        };
        JArray::from_gpu_complex(gb, a.shape.clone())
    } else {
        let gb = GpuBuffer {
            count:     a.count,
            elem_type: crate::gpu::GpuElemType::Float,
            inner: crate::gpu::GpuBufferInner {
                buffer: result_buf,
                size:   result_size,
            },
        };
        JArray::from_gpu_float(gb, a.shape.clone())
    };

    Ok(result_gpu_buf)
}

// ─────────────────────────────────────────
// i. (iota)
// ─────────────────────────────────────────

pub struct Iota;

impl Verb for Iota {
    // supports_gpu = false (구조 동사, GPU 불필요)
    fn monad(&self, _: &Interpreter, w: &JVal) -> JResult<JVal> {
        match w.as_int() {
            Some(v) => {
                if w.rank == 0 {
                    let n = v[0];
                    if n < 0 { return Err(domain_err("i. requires non-negative integer")); }
                    return Ok(JArray::vector_int((0..n).collect()));
                }
                if w.rank == 1 {
                    let shape: Vec<usize> = v.iter()
                        .map(|&x| if x < 0 { 0 } else { x as usize }).collect();
                    let count: usize = shape.iter().product::<usize>().max(1);
                    return Ok(JArray::array_int(shape, (0..count as i64).collect()));
                }
                Err(domain_err("i. requires scalar or vector argument"))
            }
            _ => Err(domain_err("i. requires integer argument")),
        }
    }
    fn dyad(&self, _:&Interpreter, _:&JVal, _:&JVal) -> JResult<JVal> {
        Err(domain_err("i. dyad not implemented"))
    }
    fn name(&self) -> &str { "i." }
}

// ─────────────────────────────────────────
// + (plus / conjugate)
// GPU: op_code = 0 (add)
// ─────────────────────────────────────────

pub struct Plus;

impl Verb for Plus {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
    // supports_gpu = false (기본값)
    // t. 없이는 CPU에서 실행
    fn gpu_dyad_op_code(&self) -> Option<u32> { Some(0) }  // add

    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(x),
            float: |&x| Ok(x),
            cmpx:  |(r,i)| Ok((r, -i))   // conjugate
        )
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(interp, a, w,
            int:   |x,y| Ok(x+y),
            float: |x,y| Ok(x+y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar+wr, ai+wi))
        )
    }
    fn name(&self) -> &str { "+" }
}

// ─────────────────────────────────────────
// - (minus / negate)
// GPU: op_code = 1 (sub)
// ─────────────────────────────────────────

pub struct Minus;

impl Verb for Minus {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
    // supports_gpu = false (기본값)
    fn gpu_dyad_op_code(&self) -> Option<u32> { Some(1) }  // sub

    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(-x),
            float: |&x| Ok(-x),
            cmpx:  |(r,i)| Ok((-r,-i))
        )
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(interp, a, w,
            int:   |x,y| Ok(x-y),
            float: |x,y| Ok(x-y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar-wr, ai-wi))
        )
    }
    fn name(&self) -> &str { "-" }
}

// ─────────────────────────────────────────
// * (times / signum)
// GPU: op_code = 2 (mul)
// ─────────────────────────────────────────

pub struct Star;

impl Verb for Star {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
    // supports_gpu = false (기본값)
    fn gpu_dyad_op_code(&self) -> Option<u32> { Some(2) }  // mul

    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        apply_monad_numeric!(w,
            int:   |&x| Ok(x.signum()),
            float: |&x:&f64| Ok(if *x==0.0 {0.0} else {x.signum()}),
            cmpx:  |(r,i):(f64,f64)| {
                let mag = (r*r+i*i).sqrt();
                if mag==0.0 {Ok((0.0,0.0))} else {Ok((r/mag,i/mag))}
            }
        )
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        apply_dyad_numeric!(interp, a, w,
            int:   |x,y| Ok(x*y),
            float: |x,y| Ok(x*y),
            cmpx:  |(ar,ai),(wr,wi)| Ok((ar*wr-ai*wi, ar*wi+ai*wr))
        )
    }
    fn name(&self) -> &str { "*" }
}

// ─────────────────────────────────────────
// % (divide / reciprocal)
// GPU: op_code = 3 (div)
// ─────────────────────────────────────────

pub struct Percent;

impl Verb for Percent {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
    // supports_gpu = false (기본값)
    fn gpu_dyad_op_code(&self) -> Option<u32> { Some(3) }  // div

    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        let fw = w.to_float();
        apply_monad_numeric!(&fw,
            int:   |_| Err(domain_err("unreachable")),
            float: |&x| if x==0.0 {
                Err(domain_err("% monad: divide by zero"))
            } else { Ok(1.0/x) },
            cmpx:  |(r,i):(f64,f64)| {
                let d=r*r+i*i;
                if d==0.0 {Err(domain_err("% monad: divide by zero"))}
                else {Ok((r/d,-i/d))}
            }
        )
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        let (pa, pw) = (a.to_float(), w.to_float());
        apply_dyad_numeric!(interp, &pa, &pw,
            int:   |_,_| Err(domain_err("unreachable")),
            float: |x,y| if y==0.0 {
                Err(domain_err("% dyad: divide by zero"))
            } else { Ok(x/y) },
            cmpx:  |(ar,ai),(wr,wi)| {
                let d=wr*wr+wi*wi;
                if d==0.0 {Err(domain_err("% dyad: divide by zero"))}
                else {Ok(((ar*wr+ai*wi)/d,(ai*wr-ar*wi)/d))}
            }
        )
    }
    fn name(&self) -> &str { "%" }
}

// ─────────────────────────────────────────
// | (residue / magnitude)
// GPU 미지원 (residue는 GPU에서 이득 없음)
// ─────────────────────────────────────────

pub struct Bar;

impl Verb for Bar {
    fn monad_rank(&self) -> i64 { 0 }
    fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
    fn supports_complex(&self) -> bool { false }

    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        match &w.data {
            JData::Integer(v) => Ok(make_int_from(v.iter().map(|&x| x.abs()).collect(), w)),
            JData::Float(v)   => Ok(make_float_from(v.iter().map(|x| x.abs()).collect(), w)),
            JData::Complex(v) => {
                let r: Vec<f64> = v.chunks(2).map(|c| (c[0]*c[0]+c[1]*c[1]).sqrt()).collect();
                Ok(make_float_from(r, w))
            }
            _ => Err(domain_err("| requires numeric argument")),
        }
    }
    fn dyad(&self, _:&Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        check_complex_dyad(self, a, w)?;
        // interp 없이 직접 CPU 경로 (| 는 항상 CPU)
        let (pa, pw) = promote_pair(a, w);
        match (&pa.data, &pw.data) {
            (JData::Integer(av), JData::Integer(wv)) => {
                let r = dyad_int(av, wv, pa.rank, pw.rank,
                    |x,y| Ok(if x==0 {y} else {y.rem_euclid(x)}))?;
                Ok(make_int(r, &pa, &pw))
            }
            (JData::Float(av), JData::Float(wv)) => {
                let r = dyad_float(av, wv, pa.rank, pw.rank,
                    |x,y| Ok(if x==0.0 {y} else {y.rem_euclid(x)}))?;
                Ok(make_float(r, &pa, &pw))
            }
            _ => Err(domain_err("| requires numeric arguments")),
        }
    }
    fn name(&self) -> &str { "|" }
}

// ─────────────────────────────────────────
// $ (shape / reshape)
// ─────────────────────────────────────────

pub struct Dollar;

impl Verb for Dollar {
    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        if w.rank == 0 { return Ok(JArray::vector_int(vec![])); }
        Ok(JArray::vector_int(w.shape.iter().map(|&x| x as i64).collect()))
    }
    fn dyad(&self, _:&Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        let sv = a.as_int().ok_or_else(|| domain_err("$ requires integer shape"))?;
        let ns: Vec<usize> = sv.iter().map(|&x| if x<0 {0usize} else {x as usize}).collect();
        let nc: usize = ns.iter().product::<usize>().max(1);
        match &w.data {
            JData::Integer(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                Ok(JArray::array_int(ns, (0..nc).map(|i| wv[i%wv.len()]).collect()))
            }
            JData::Float(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                Ok(JArray::array_float(ns, (0..nc).map(|i| wv[i%wv.len()]).collect()))
            }
            JData::Complex(wv) => {
                if wv.is_empty() { return Err(domain_err("$ reshape: empty source")); }
                let pc = wv.len()/2;
                let flat: Vec<f64> = (0..nc)
                    .flat_map(|i| { let p=i%pc; [wv[p*2],wv[p*2+1]] }).collect();
                Ok(JArray::array_complex(ns, flat))
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
    fn monad(&self, _:&Interpreter, w: &JVal) -> JResult<JVal> {
        Ok(JArray::scalar_int(w.tally() as i64))
    }
    fn dyad(&self, _:&Interpreter, _:&JVal, _:&JVal) -> JResult<JVal> {
        Err(domain_err("# dyad not implemented"))
    }
    fn name(&self) -> &str { "#" }
}

// ─────────────────────────────────────────
// 비교 동사 - Complex 불가, GPU 불가
// ─────────────────────────────────────────

pub struct Lt; pub struct Gt;
pub struct Le; pub struct Ge;
pub struct Eq; pub struct Ne;

macro_rules! impl_cmp_verb {
    ($t:ty, $sym:expr, $oi:expr, $of:expr) => {
        impl Verb for $t {
            fn monad_rank(&self) -> i64 { 0 }
            fn dyad_rank(&self)  -> (i64, i64) { (0, 0) }
            fn supports_complex(&self) -> bool { false }
            // supports_gpu = false (기본값)

            fn monad(&self, _:&Interpreter, _:&JVal) -> JResult<JVal> {
                Err(domain_err(concat!($sym, " monad not implemented")))
            }
            fn dyad(&self, _:&Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
                check_complex_dyad(self, a, w)?;
                let (pa, pw) = promote_pair(a, w);
                match (&pa.data, &pw.data) {
                    (JData::Integer(av), JData::Integer(wv)) => {
                        let r = dyad_int(av, wv, pa.rank, pw.rank,
                            |x,y| Ok(if $oi(x,y) {1i64} else {0i64}))?;
                        Ok(make_int(r, &pa, &pw))
                    }
                    (JData::Float(av), JData::Float(wv)) => {
                        let r = dyad_float(av, wv, pa.rank, pw.rank,
                            |x,y| Ok(if $of(x,y) {1.0f64} else {0.0f64}))?;
                        // Float 비교 결과를 Integer로
                        let iv: Vec<i64> = r.iter().map(|&x| x as i64).collect();
                        Ok(make_int(iv, &pa, &pw))
                    }
                    _ => Err(domain_err(concat!($sym, ": unsupported type"))),
                }
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
// GPU: +/ 와 */ 는 reduce 커널 사용
// ─────────────────────────────────────────

pub struct Slash { pub u: VerbBox }

impl Verb for Slash {
    /// u/ の monad rank = u の dyad rrank + 1
    ///
    /// 근거:
    ///   u/ 는 "rank r 의 cell들에 u를 fold"
    ///   u 가 rank r 의 dyad라면
    ///   u/ 는 rank r+1 의 배열을 받아 각 rank-r cell에 fold
    ///
    /// 예:
    ///   +  dyad rrank = 0  →  +/ monad rank = 1  (벡터에 fold)
    ///
    /// 파싱 시점에 정적으로 결정됨 (실행 전)
    fn monad_rank(&self) -> i64 {
        let rrank = self.u.dyad_rank().1;
        if rrank == i64::MAX { i64::MAX } else { rrank + 1 }
    }

    fn supports_gpu(&self) -> bool { self.u.supports_gpu() }

    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // GPU reduce 경로
        #[cfg(feature = "gpu")]
        if let Backend::Gpu(dev) = &interp.backend {
            if self.u.supports_gpu() {
                let wg = w.to_gpu(dev);
                return gpu_reduce(dev, &wg, self.u.gpu_dyad_op_code());
            }
        }

        // CPU 경로: 오른쪽에서 왼쪽으로 fold
        let tally = w.tally();
        if tally == 0 { return Err(domain_err("u/ requires non-empty array")); }
        let cell_size = w.count / tally;
        let mut result = extract_cell(w, tally - 1, cell_size);
        for i in (0..tally - 1).rev() {
            let left = extract_cell(w, i, cell_size);
            result = self.u.dyad(interp, &left, &result)?;
        }
        Ok(result)
    }

    fn dyad(&self, _:&Interpreter, _:&JVal, _:&JVal) -> JResult<JVal> {
        Err(domain_err("/ dyad not implemented"))
    }
    fn name(&self) -> &str { "/" }
}

/// GPU reduce: +/, */ 등
/// reduce 커널이 전체 배열을 병렬로 fold
#[cfg(feature = "gpu")]
fn gpu_reduce(
    dev:     &crate::gpu::GpuDevice,
    w:       &JVal,
    op_code: Option<u32>,
) -> JResult<JVal> {
    // op_code: 0=sum, 1=prod (reduce 커널 선택)
    // 실제 구현에서는 multi-pass reduction 필요
    // (workgroup 크기 64, 나머지는 다음 pass)
    // 여기서는 구조만 표현
    Err(domain_err("GPU reduce not yet implemented"))
}

// ─────────────────────────────────────────
// t. (task conjunction)
//
// J의 원래 의미: "Execute as task"
//   원래: 별도 OS 스레드에서 실행
//   확장: GPU가 있으면 GPU 커널, 없으면 스레드
//
// 설계 원칙:
//   supports_gpu()는 기본값이 false
//   t. 로 감싸야만 true → 사용자가 명시적으로 선택
//
// 사용 예:
//   +t.       NB. + 를 태스크로
//   (+t.)/    NB. +/ 를 GPU reduce로
//   mean_g =: (+t.) / % #
// ─────────────────────────────────────────

pub struct Tco { pub u: VerbBox }

impl Verb for Tco {
    // rank는 내부 동사에서 그대로 위임
    fn monad_rank(&self) -> i64 { self.u.monad_rank() }
    fn dyad_rank(&self)  -> (i64, i64) { self.u.dyad_rank() }

    fn supports_complex(&self) -> bool { self.u.supports_complex() }

    /// t. 로 감싸인 동사만 GPU 실행
    fn supports_gpu(&self) -> bool { true }

    /// 내부 동사의 op_code를 그대로 전달
    /// rank1ex/rank2ex 가 커널 선택에 사용
    fn gpu_dyad_op_code(&self) -> Option<u32> { self.u.gpu_dyad_op_code() }

    /// 실행은 내부 동사에 위임
    /// rank1ex/rank2ex 가 backend 보고 GPU/CPU 경로 선택
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        self.u.monad(interp, w)
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        self.u.dyad(interp, a, w)
    }
    fn name(&self) -> &str { "t." }
}

// ─────────────────────────────────────────
// GPU: f, h의 중간 결과가 VRAM에 머묾
//      g 적용 시 전송 없이 바로 처리
// ─────────────────────────────────────────

pub struct Fork { pub f: VerbBox, pub g: VerbBox, pub h: VerbBox }

impl Verb for Fork {
    /// fork 전체의 monad rank = min(f.monad_rank, h.monad_rank)
    ///
    /// 근거:
    ///   fork(f, g, h) w  →  (f w) g (h w)
    ///   f와 h 중 더 세밀하게 w를 분해해야 하는 쪽에 맞춰야 함
    ///   더 작은 rank = 더 세밀한 분해
    ///   → w를 min rank의 cell로 한 번만 분해하면
    ///     f와 h 모두 자신의 rank 조건을 만족
    ///
    /// 예:
    ///   mean =: +/ % #
    ///   +/ monad rank = 1
    ///   #  monad rank = ∞
    ///   fork rank = min(1, ∞) = 1
    ///   → w를 rank-1 cell로 한 번만 분해
    ///   → +/ 와 # 모두 그 cell을 받음
    ///   → +/ 는 벡터 전체를 fold, # 는 벡터 전체를 tally
    ///
    ///   (+ * -) i. 2 3
    ///   +, *, - monad rank = 0
    ///   fork rank = min(0, 0) = 0
    ///   → w를 스칼라로 한 번만 분해
    ///   → 세 동사 모두 같은 cell을 받음
    ///
    /// 파싱 시점에 정적으로 결정됨
    fn monad_rank(&self) -> i64 {
        self.f.monad_rank().min(self.h.monad_rank())
    }

    /// fork 전체의 dyad rank = (min(f.lrank, h.lrank), min(f.rrank, h.rrank))
    fn dyad_rank(&self) -> (i64, i64) {
        let (fl, fr) = self.f.dyad_rank();
        let (hl, hr) = self.h.dyad_rank();
        (fl.min(hl), fr.min(hr))
    }

    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        // fork 자신의 rank로 w를 한 번만 분해
        // rank1ex가 self.monad_rank()를 사용 → f, h에 같은 cell이 전달됨
        // (지금은 rank1ex가 외부에서 이미 호출하므로
        //  여기서는 cell이 들어온 상태 - f, h에 직접 전달)
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
// GPU 경로:
//   supports_gpu() = true 이면 cell 분해 없이
//   전체 배열을 GPU로 올리고 커널이 한 번에 처리
//
// CPU 경로:
//   기존과 동일하게 cell 단위로 분해
// ─────────────────────────────────────────

pub fn rank1ex(verb: &dyn Verb, interp: &Interpreter,
               w: &JVal, verb_rank: i64) -> JResult<JVal>
{
    // GPU 경로: cell 분해 없이 전체를 한 번에
    #[cfg(feature = "gpu")]
    if let Backend::Gpu(dev) = &interp.backend {
        if verb.supports_gpu() {
            let wg = w.to_gpu(dev);
            return verb.monad(interp, &wg);
        }
        // GPU 미지원 동사: CPU로 폴백
        let wc = w.to_cpu(dev);
        return rank1ex_cpu(verb, interp, &wc, verb_rank);
    }

    rank1ex_cpu(verb, interp, w, verb_rank)
}

fn rank1ex_cpu(verb: &dyn Verb, interp: &Interpreter,
               w: &JVal, verb_rank: i64) -> JResult<JVal>
{
    if verb_rank == i64::MAX || w.rank as i64 <= verb_rank {
        return verb.monad(interp, w);
    }
    let leading   = w.tally();
    let cell_size = w.count / leading;
    let mut results: Vec<JVal> = Vec::with_capacity(leading);
    for i in 0..leading {
        results.push(verb.monad(interp, &extract_cell(w, i, cell_size))?);
    }
    let cell_rank = verb_rank.max(0) as usize;
    let frame = &w.shape[..w.rank - cell_rank.min(w.rank)];
    assemble_results(results, leading, frame)
}

pub fn rank2ex(verb: &dyn Verb, interp: &Interpreter,
               a: &JVal, w: &JVal, lrank: i64, rrank: i64) -> JResult<JVal>
{
    // GPU 경로
    #[cfg(feature = "gpu")]
    if let Backend::Gpu(dev) = &interp.backend {
        if verb.supports_gpu() {
            let ag = a.to_gpu(dev);
            let wg = w.to_gpu(dev);
            return verb.dyad(interp, &ag, &wg);
        }
        let ac = a.to_cpu(dev);
        let wc = w.to_cpu(dev);
        return rank2ex_cpu(verb, interp, &ac, &wc, lrank, rrank);
    }

    rank2ex_cpu(verb, interp, a, w, lrank, rrank)
}

fn rank2ex_cpu(verb: &dyn Verb, interp: &Interpreter,
               a: &JVal, w: &JVal, lrank: i64, rrank: i64) -> JResult<JVal>
{
    let a_ok = lrank == i64::MAX || a.rank as i64 <= lrank;
    let w_ok = rrank == i64::MAX || w.rank as i64 <= rrank;
    if a_ok && w_ok { return verb.dyad(interp, a, w); }

    let eff_l = effective_rank(a.rank, lrank);
    let eff_r = effective_rank(w.rank, rrank);
    let al    = if eff_l >= a.rank { 1 } else { a.shape[0] };
    let wl    = if eff_r >= w.rank { 1 } else { w.shape[0] };

    if al != wl && al != 1 && wl != 1 {
        return Err(JError::no_loc(JErrorKind::Length,
            format!("rank agreement: frame mismatch {} vs {}", al, wl)));
    }

    let leading     = al.max(wl);
    let a_cell_size = a.count / al;
    let w_cell_size = w.count / wl;

    let mut results: Vec<JVal> = Vec::with_capacity(leading);
    for i in 0..leading {
        let ai = if al == 1 { 0 } else { i };
        let wi = if wl == 1 { 0 } else { i };
        results.push(verb.dyad(interp,
            &extract_cell(a, ai, a_cell_size),
            &extract_cell(w, wi, w_cell_size)
        )?);
    }

    let frame = if a.rank > eff_l { &a.shape[..a.rank-eff_l] }
                else if w.rank > eff_r { &w.shape[..w.rank-eff_r] }
                else { &[] };
    assemble_results(results, leading, frame)
}

fn effective_rank(arg_rank: usize, verb_rank: i64) -> usize {
    if verb_rank == i64::MAX { arg_rank }
    else if verb_rank >= 0 { (verb_rank as usize).min(arg_rank) }
    else { (arg_rank as i64 + verb_rank).max(0) as usize }
}

// ─────────────────────────────────────────
// cell 추출 / 결과 조립
// ─────────────────────────────────────────

pub fn extract_cell(w: &JVal, i: usize, cell_size: usize) -> JVal {
    let start = i * cell_size;
    let end   = start + cell_size;
    let shape = if w.rank <= 1 { vec![] } else { w.shape[1..].to_vec() };

    match &w.data {
        JData::Integer(v) => {
            let s = v[start..end].to_vec();
            if shape.is_empty() { JArray::scalar_int(s[0]) }
            else if shape.len()==1 { JArray::vector_int(s) }
            else { JArray::array_int(shape, s) }
        }
        JData::Float(v) => {
            let s = v[start..end].to_vec();
            if shape.is_empty() { JArray::scalar_float(s[0]) }
            else if shape.len()==1 { JArray::vector_float(s) }
            else { JArray::array_float(shape, s) }
        }
        JData::Complex(v) => {
            let s = v[start*2..end*2].to_vec();
            if shape.is_empty() { JArray::scalar_complex(s[0], s[1]) }
            else if shape.len()==1 { JArray::array_complex(vec![cell_size], s) }
            else { JArray::array_complex(shape, s) }
        }
        _ => panic!("extract_cell: non-numeric type"),
    }
}

fn assemble_results(results: Vec<JVal>, leading: usize, frame: &[usize]) -> JResult<JVal> {
    if results.is_empty() { return Ok(JArray::vector_int(vec![])); }
    if leading == 1 && frame.is_empty() { return Ok(Arc::clone(&results[0])); }

    let rs = &results[0].shape;
    let mut fs: Vec<usize> = frame.to_vec();
    fs.extend_from_slice(rs);

    match &results[0].data {
        JData::Integer(_) => {
            let mut flat: Vec<i64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_int()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            if fs.is_empty() { Ok(JArray::scalar_int(flat[0])) }
            else if fs.len()==1 { Ok(JArray::vector_int(flat)) }
            else { Ok(JArray::array_int(fs, flat)) }
        }
        JData::Float(_) => {
            let mut flat: Vec<f64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_float()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            if fs.is_empty() { Ok(JArray::scalar_float(flat[0])) }
            else if fs.len()==1 { Ok(JArray::vector_float(flat)) }
            else { Ok(JArray::array_float(fs, flat)) }
        }
        JData::Complex(_) => {
            let mut flat: Vec<f64> = Vec::new();
            for r in &results {
                flat.extend_from_slice(r.as_complex()
                    .ok_or_else(|| domain_err("assemble: type mismatch"))?);
            }
            let count = flat.len()/2;
            if fs.is_empty() { Ok(JArray::scalar_complex(flat[0], flat[1])) }
            else if fs.len()==1 { Ok(JArray::array_complex(vec![count], flat)) }
            else { Ok(JArray::array_complex(fs, flat)) }
        }
        _ => Err(domain_err("assemble: non-numeric result")),
    }
}
