use std::fmt;
use std::sync::Arc;

/// J의 numeric tower
/// Integer < Float < Complex
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumericType {
    Integer,
    Float,
    Complex,
}

/// J의 AT 필드에 해당
#[derive(Debug, Clone, PartialEq)]
pub enum JType {
    Numeric(NumericType),
    Verb,
    Adverb,
}

pub type VerbBox = Arc<dyn crate::verbs::Verb>;

/// J의 A 블록
/// CPU와 GPU 데이터를 모두 담을 수 있음
///
/// GPU 경로의 핵심 설계:
///   JData::GpuFloat / GpuComplex 를 담은 JVal이
///   중간 결과로 VRAM에 머물 수 있음
///   → 연산 체인에서 CPU↔GPU 전송 최소화
#[derive(Clone)]
pub struct JArray {
    pub typ:   JType,
    pub rank:  usize,
    pub shape: Vec<usize>,
    pub count: usize,
    pub data:  JData,
}

#[derive(Clone)]
pub enum JData {
    // ── CPU 데이터 ──
    Integer(Vec<i64>),
    Float(Vec<f64>),
    /// flat [r0,i0,r1,i1,...] J와 동일
    Complex(Vec<f64>),
    Verb(VerbBox),

    // ── GPU 데이터 (VRAM) ──
    // feature = "gpu" 없으면 이 변형을 만들 수 없음
    // Arc: 같은 버퍼를 여러 JVal이 공유 가능 (복사 없이)
    #[cfg(feature = "gpu")]
    GpuFloat(Arc<crate::gpu::GpuBuffer>),

    #[cfg(feature = "gpu")]
    GpuComplex(Arc<crate::gpu::GpuBuffer>),
}

pub type JVal = Arc<JArray>;

impl JArray {
    // ─────────────────────────────────────────
    // CPU 생성자
    // ─────────────────────────────────────────

    pub fn scalar_int(n: i64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank:  0, shape: vec![], count: 1,
            data:  JData::Integer(vec![n]),
        })
    }

    pub fn vector_int(v: Vec<i64>) -> JVal {
        let n = v.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank:  1, shape: vec![n], count: n,
            data:  JData::Integer(v),
        })
    }

    pub fn array_int(shape: Vec<usize>, data: Vec<i64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank, shape, count,
            data:  JData::Integer(data),
        })
    }

    pub fn scalar_float(x: f64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank:  0, shape: vec![], count: 1,
            data:  JData::Float(vec![x]),
        })
    }

    pub fn vector_float(v: Vec<f64>) -> JVal {
        let n = v.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank:  1, shape: vec![n], count: n,
            data:  JData::Float(v),
        })
    }

    pub fn array_float(shape: Vec<usize>, data: Vec<f64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank, shape, count,
            data:  JData::Float(data),
        })
    }

    pub fn scalar_complex(r: f64, i: f64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank:  0, shape: vec![], count: 1,
            data:  JData::Complex(vec![r, i]),
        })
    }

    pub fn vector_complex(v: Vec<(f64, f64)>) -> JVal {
        let n = v.len();
        let flat: Vec<f64> = v.into_iter().flat_map(|(r,i)| [r,i]).collect();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank:  1, shape: vec![n], count: n,
            data:  JData::Complex(flat),
        })
    }

    pub fn array_complex(shape: Vec<usize>, flat: Vec<f64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank, shape, count,
            data:  JData::Complex(flat),
        })
    }

    pub fn from_verb(verb: VerbBox) -> JVal {
        Arc::new(JArray {
            typ:   JType::Verb,
            rank:  0, shape: vec![], count: 1,
            data:  JData::Verb(verb),
        })
    }

    // ─────────────────────────────────────────
    // GPU 생성자 - feature = "gpu" 일 때만 존재
    // ─────────────────────────────────────────

    #[cfg(feature = "gpu")]
    pub fn from_gpu_float(
        buf:   crate::gpu::GpuBuffer,
        shape: Vec<usize>,
    ) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank, shape, count,
            data:  JData::GpuFloat(Arc::new(buf)),
        })
    }

    #[cfg(feature = "gpu")]
    pub fn from_gpu_complex(
        buf:   crate::gpu::GpuBuffer,
        shape: Vec<usize>,
    ) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank, shape, count,
            data:  JData::GpuComplex(Arc::new(buf)),
        })
    }

    // ─────────────────────────────────────────
    // 데이터 접근
    // ─────────────────────────────────────────

    pub fn as_int(&self) -> Option<&Vec<i64>> {
        match &self.data { JData::Integer(v) => Some(v), _ => None }
    }

    pub fn as_float(&self) -> Option<&Vec<f64>> {
        match &self.data { JData::Float(v) => Some(v), _ => None }
    }

    pub fn as_complex(&self) -> Option<&Vec<f64>> {
        match &self.data { JData::Complex(v) => Some(v), _ => None }
    }

    pub fn as_verb(&self) -> Option<&VerbBox> {
        match &self.data { JData::Verb(v) => Some(v), _ => None }
    }

    pub fn is_verb(&self)   -> bool { self.typ == JType::Verb }
    pub fn is_scalar(&self) -> bool { self.rank == 0 }

    pub fn numeric_type(&self) -> Option<&NumericType> {
        match &self.typ { JType::Numeric(t) => Some(t), _ => None }
    }

    /// GPU에 있는 데이터인지 확인
    pub fn is_on_gpu(&self) -> bool {
        #[cfg(feature = "gpu")]
        matches!(self.data, JData::GpuFloat(_) | JData::GpuComplex(_));
        false
    }

    // ─────────────────────────────────────────
    // CPU ↔ GPU 변환
    // ─────────────────────────────────────────

    /// CPU 데이터를 GPU로 업로드
    /// GPU에 이미 있으면 그대로 반환 (복사 없음)
    /// "처음 한 번만 GPU로" 원칙
    #[cfg(feature = "gpu")]
    pub fn to_gpu(&self, dev: &crate::gpu::GpuDevice) -> JVal {
        match &self.data {
            JData::GpuFloat(_) | JData::GpuComplex(_) => Arc::new(self.clone()),
            JData::Float(v) => {
                let buf = crate::gpu::GpuBuffer::from_cpu_float(dev, v);
                JArray::from_gpu_float(buf, self.shape.clone())
            }
            JData::Complex(v) => {
                let buf = crate::gpu::GpuBuffer::from_cpu_complex(dev, v);
                JArray::from_gpu_complex(buf, self.shape.clone())
            }
            JData::Integer(v) => {
                // Integer → Float으로 승격 후 GPU로
                let fv: Vec<f64> = v.iter().map(|&x| x as f64).collect();
                let buf = crate::gpu::GpuBuffer::from_cpu_float(dev, &fv);
                JArray::from_gpu_float(buf, self.shape.clone())
            }
            _ => panic!("to_gpu: verb cannot be on GPU"),
        }
    }

    /// GPU 데이터를 CPU로 다운로드
    /// CPU에 이미 있으면 그대로 반환 (복사 없음)
    /// "출력 시에만 CPU로" 원칙
    #[cfg(feature = "gpu")]
    pub fn to_cpu(&self, dev: &crate::gpu::GpuDevice) -> JVal {
        match &self.data {
            JData::GpuFloat(buf) => {
                let v = buf.to_cpu(dev);
                JArray::array_float(self.shape.clone(), v)
            }
            JData::GpuComplex(buf) => {
                let v = buf.to_cpu(dev);
                JArray::array_complex(self.shape.clone(), v)
            }
            _ => Arc::new(self.clone()),
        }
    }

    // ─────────────────────────────────────────
    // 타입 승격
    // ─────────────────────────────────────────

    pub fn to_float(&self) -> JVal {
        match &self.data {
            JData::Float(_) => Arc::new(self.clone()),
            JData::Integer(v) => {
                let fv: Vec<f64> = v.iter().map(|&x| x as f64).collect();
                if self.rank == 0 { JArray::scalar_float(fv[0]) }
                else if self.rank == 1 { JArray::vector_float(fv) }
                else { JArray::array_float(self.shape.clone(), fv) }
            }
            _ => panic!("to_float: not a CPU numeric type"),
        }
    }

    pub fn to_complex(&self) -> JVal {
        match &self.data {
            JData::Complex(_) => Arc::new(self.clone()),
            JData::Float(v) => {
                let flat: Vec<f64> = v.iter().flat_map(|&r| [r, 0.0]).collect();
                if self.rank == 0 { JArray::scalar_complex(v[0], 0.0) }
                else if self.rank == 1 {
                    JArray::vector_complex(v.iter().map(|&r| (r, 0.0)).collect())
                } else { JArray::array_complex(self.shape.clone(), flat) }
            }
            JData::Integer(v) => {
                let flat: Vec<f64> = v.iter().flat_map(|&r| [r as f64, 0.0]).collect();
                if self.rank == 0 { JArray::scalar_complex(v[0] as f64, 0.0) }
                else if self.rank == 1 {
                    JArray::vector_complex(v.iter().map(|&r| (r as f64, 0.0)).collect())
                } else { JArray::array_complex(self.shape.clone(), flat) }
            }
            _ => panic!("to_complex: not a CPU numeric type"),
        }
    }

    // ─────────────────────────────────────────
    // shape 헬퍼
    // ─────────────────────────────────────────

    pub fn flat_index(&self, indices: &[usize]) -> usize {
        indices.iter().zip(self.shape.iter())
            .fold(0, |acc, (&idx, &dim)| acc * dim + idx)
    }

    pub fn tally(&self) -> usize {
        if self.rank == 0 { 1 } else { self.shape[0] }
    }
}

// ─────────────────────────────────────────
// Display
// GPU 데이터는 출력 전에 to_cpu() 필요
// ─────────────────────────────────────────

impl fmt::Display for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            JData::Integer(v) => fmt_numeric(f, v, &self.shape, self.rank, |x| x.to_string()),
            JData::Float(v)   => fmt_numeric(f, v, &self.shape, self.rank, fmt_f64),
            JData::Complex(v) => fmt_complex(f, v, &self.shape, self.rank),
            JData::Verb(v)    => write!(f, "(verb:{})", v.name()),
            #[cfg(feature = "gpu")]
            JData::GpuFloat(_) | JData::GpuComplex(_) =>
                write!(f, "<gpu array shape={:?}>", self.shape),
        }
    }
}

fn fmt_f64(x: &f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", *x as i64)
    } else {
        format!("{}", x)
    }
}

fn fmt_numeric<T>(f: &mut fmt::Formatter<'_>, v: &[T],
                  shape: &[usize], rank: usize,
                  to_str: impl Fn(&T) -> String) -> fmt::Result {
    match rank {
        0 => write!(f, "{}", to_str(&v[0])),
        1 => write!(f, "{}", v.iter().map(&to_str).collect::<Vec<_>>().join(" ")),
        2 => {
            for (i, row) in v.chunks(shape[1]).enumerate() {
                if i > 0 { writeln!(f)?; }
                write!(f, "{}", row.iter().map(&to_str).collect::<Vec<_>>().join(" "))?;
            }
            Ok(())
        }
        _ => {
            let ms: usize = shape[rank-2..].iter().product();
            for (i, matrix) in v.chunks(ms).enumerate() {
                if i > 0 { writeln!(f)?; writeln!(f)?; }
                for (j, row) in matrix.chunks(shape[rank-1]).enumerate() {
                    if j > 0 { writeln!(f)?; }
                    write!(f, "{}", row.iter().map(&to_str).collect::<Vec<_>>().join(" "))?;
                }
            }
            Ok(())
        }
    }
}

fn fmt_complex(f: &mut fmt::Formatter<'_>, v: &[f64],
               shape: &[usize], rank: usize) -> fmt::Result {
    let pairs: Vec<String> = v.chunks(2).map(|c| {
        let r = c[0]; let i = c[1];
        if i == 0.0 { fmt_f64(&r) }
        else { format!("{}j{}", fmt_f64(&r), fmt_f64(&i)) }
    }).collect();
    fmt_numeric(f, &pairs, shape, rank, |s| s.clone())
}

impl fmt::Debug for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JArray(rank={}, shape={:?}, type={:?})", self.rank, self.shape, self.typ)
    }
}
