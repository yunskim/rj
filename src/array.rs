use std::fmt;
use std::sync::Arc;

/// J의 numeric tower
/// Integer < Float < Complex
/// 승격 시 더 큰 타입으로 변환
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

/// Verb trait을 Arc로 감싼 타입
pub type VerbBox = Arc<dyn crate::verbs::Verb>;

/// J의 A 블록에 해당
/// 데이터는 항상 flat Vec (J와 동일)
/// Complex는 J처럼 f64 두 개가 한 원소: [r0, i0, r1, i1, ...]
#[derive(Clone)]
pub struct JArray {
    pub typ:   JType,
    pub rank:  usize,
    pub shape: Vec<usize>,
    pub count: usize,      // 원소 수 (Complex는 f64 쌍 수)
    pub data:  JData,
}

#[derive(Clone)]
pub enum JData {
    Integer(Vec<i64>),
    Float(Vec<f64>),
    /// J와 동일: flat [r0, i0, r1, i1, ...]
    /// count = len / 2
    Complex(Vec<f64>),
    Verb(VerbBox),
}

pub type JVal = Arc<JArray>;

impl JArray {
    // ─────────────────────────────────────────
    // 생성자 - Integer
    // ─────────────────────────────────────────

    pub fn scalar_int(n: i64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Integer(vec![n]),
        })
    }

    pub fn vector_int(v: Vec<i64>) -> JVal {
        let n = v.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank:  1,
            shape: vec![n],
            count: n,
            data:  JData::Integer(v),
        })
    }

    pub fn array_int(shape: Vec<usize>, data: Vec<i64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        assert_eq!(data.len(), count,
            "data length {} != shape product {}", data.len(), count);
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Integer),
            rank,
            shape,
            count,
            data:  JData::Integer(data),
        })
    }

    // ─────────────────────────────────────────
    // 생성자 - Float
    // ─────────────────────────────────────────

    pub fn scalar_float(x: f64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Float(vec![x]),
        })
    }

    pub fn vector_float(v: Vec<f64>) -> JVal {
        let n = v.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank:  1,
            shape: vec![n],
            count: n,
            data:  JData::Float(v),
        })
    }

    pub fn array_float(shape: Vec<usize>, data: Vec<f64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Float),
            rank,
            shape,
            count,
            data:  JData::Float(data),
        })
    }

    // ─────────────────────────────────────────
    // 생성자 - Complex
    // J 표기: 3j4 → real=3.0, imag=4.0
    // flat 저장: [r, i]
    // ─────────────────────────────────────────

    pub fn scalar_complex(r: f64, i: f64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Complex(vec![r, i]),
        })
    }

    pub fn vector_complex(v: Vec<(f64, f64)>) -> JVal {
        let n = v.len();
        let flat: Vec<f64> = v.into_iter().flat_map(|(r, i)| [r, i]).collect();
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank:  1,
            shape: vec![n],
            count: n,
            data:  JData::Complex(flat),
        })
    }

    pub fn array_complex(shape: Vec<usize>, flat: Vec<f64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank  = shape.len();
        assert_eq!(flat.len(), count * 2,
            "complex flat length {} != count*2 {}", flat.len(), count * 2);
        Arc::new(JArray {
            typ:   JType::Numeric(NumericType::Complex),
            rank,
            shape,
            count,
            data:  JData::Complex(flat),
        })
    }

    // ─────────────────────────────────────────
    // 생성자 - Verb
    // ─────────────────────────────────────────

    pub fn from_verb(verb: VerbBox) -> JVal {
        Arc::new(JArray {
            typ:   JType::Verb,
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Verb(verb),
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

    /// complex flat 벡터 반환 [r0, i0, r1, i1, ...]
    pub fn as_complex(&self) -> Option<&Vec<f64>> {
        match &self.data { JData::Complex(v) => Some(v), _ => None }
    }

    pub fn as_verb(&self) -> Option<&VerbBox> {
        match &self.data { JData::Verb(v) => Some(v), _ => None }
    }

    pub fn is_verb(&self) -> bool { self.typ == JType::Verb }
    pub fn is_scalar(&self) -> bool { self.rank == 0 }

    pub fn numeric_type(&self) -> Option<&NumericType> {
        match &self.typ {
            JType::Numeric(t) => Some(t),
            _ => None,
        }
    }

    // ─────────────────────────────────────────
    // 타입 승격 (numeric tower)
    // Integer → Float → Complex
    // ─────────────────────────────────────────

    /// 이 배열을 Float으로 승격
    pub fn to_float(&self) -> JVal {
        match &self.data {
            JData::Float(_) => Arc::new(self.clone()),
            JData::Integer(v) => {
                let fv: Vec<f64> = v.iter().map(|&x| x as f64).collect();
                if self.rank == 0 {
                    JArray::scalar_float(fv[0])
                } else if self.rank == 1 {
                    JArray::vector_float(fv)
                } else {
                    JArray::array_float(self.shape.clone(), fv)
                }
            }
            _ => panic!("to_float: not a numeric type"),
        }
    }

    /// 이 배열을 Complex로 승격
    pub fn to_complex(&self) -> JVal {
        match &self.data {
            JData::Complex(_) => Arc::new(self.clone()),
            JData::Float(v) => {
                let flat: Vec<f64> = v.iter().flat_map(|&r| [r, 0.0]).collect();
                if self.rank == 0 {
                    JArray::scalar_complex(v[0], 0.0)
                } else if self.rank == 1 {
                    JArray::vector_complex(v.iter().map(|&r| (r, 0.0)).collect())
                } else {
                    JArray::array_complex(self.shape.clone(), flat)
                }
            }
            JData::Integer(v) => {
                let flat: Vec<f64> = v.iter().flat_map(|&r| [r as f64, 0.0]).collect();
                if self.rank == 0 {
                    JArray::scalar_complex(v[0] as f64, 0.0)
                } else if self.rank == 1 {
                    JArray::vector_complex(v.iter().map(|&r| (r as f64, 0.0)).collect())
                } else {
                    JArray::array_complex(self.shape.clone(), flat)
                }
            }
            _ => panic!("to_complex: not a numeric type"),
        }
    }

    // ─────────────────────────────────────────
    // rank / shape 헬퍼
    // ─────────────────────────────────────────

    pub fn flat_index(&self, indices: &[usize]) -> usize {
        indices.iter().zip(self.shape.iter())
            .fold(0, |acc, (&idx, &dim)| acc * dim + idx)
    }

    pub fn multi_index(&self, mut flat: usize) -> Vec<usize> {
        let mut indices = vec![0usize; self.rank];
        for i in (0..self.rank).rev() {
            indices[i] = flat % self.shape[i];
            flat /= self.shape[i];
        }
        indices
    }

    pub fn tally(&self) -> usize {
        if self.rank == 0 { 1 } else { self.shape[0] }
    }
}

// ─────────────────────────────────────────
// Display
// ─────────────────────────────────────────

impl fmt::Display for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            JData::Integer(v) => fmt_numeric(f, v, &self.shape, self.rank,
                                             |x| x.to_string()),
            JData::Float(v)   => fmt_numeric(f, v, &self.shape, self.rank,
                                             fmt_float),
            JData::Complex(v) => fmt_complex_array(f, v, &self.shape, self.rank),
            JData::Verb(v)    => write!(f, "(verb:{})", v.name()),
        }
    }
}

fn fmt_float(x: &f64) -> String {
    // J 스타일: 정수면 소수점 없이, 아니면 표준 표기
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
        1 => {
            let s: Vec<String> = v.iter().map(&to_str).collect();
            write!(f, "{}", s.join(" "))
        }
        2 => {
            let cols = shape[1];
            for (i, row) in v.chunks(cols).enumerate() {
                if i > 0 { writeln!(f)?; }
                let s: Vec<String> = row.iter().map(&to_str).collect();
                write!(f, "{}", s.join(" "))?;
            }
            Ok(())
        }
        _ => {
            let matrix_size: usize = shape[rank-2..].iter().product();
            for (i, matrix) in v.chunks(matrix_size).enumerate() {
                if i > 0 { writeln!(f)?; writeln!(f)?; }
                let cols = shape[rank-1];
                for (j, row) in matrix.chunks(cols).enumerate() {
                    if j > 0 { writeln!(f)?; }
                    let s: Vec<String> = row.iter().map(&to_str).collect();
                    write!(f, "{}", s.join(" "))?;
                }
            }
            Ok(())
        }
    }
}

fn fmt_complex_array(f: &mut fmt::Formatter<'_>, v: &[f64],
                     shape: &[usize], rank: usize) -> fmt::Result {
    // complex flat [r0,i0,r1,i1,...] → "3j4 1j2 ..."
    let pairs: Vec<String> = v.chunks(2).map(|c| {
        let r = c[0]; let i = c[1];
        if i == 0.0 { fmt_float(&r) }
        else if i < 0.0 { format!("{}j{}", fmt_float(&r), fmt_float(&i)) }
        else { format!("{}j{}", fmt_float(&r), fmt_float(&i)) }
    }).collect();

    // pairs를 shape에 맞게 출력
    match rank {
        0 => write!(f, "{}", pairs[0]),
        1 => write!(f, "{}", pairs.join(" ")),
        2 => {
            let cols = shape[1];
            for (i, row) in pairs.chunks(cols).enumerate() {
                if i > 0 { writeln!(f)?; }
                write!(f, "{}", row.join(" "))?;
            }
            Ok(())
        }
        _ => {
            let matrix_size: usize = shape[rank-2..].iter().product();
            for (i, matrix) in pairs.chunks(matrix_size).enumerate() {
                if i > 0 { writeln!(f)?; writeln!(f)?; }
                let cols = shape[rank-1];
                for (j, row) in matrix.chunks(cols).enumerate() {
                    if j > 0 { writeln!(f)?; }
                    write!(f, "{}", row.join(" "))?;
                }
            }
            Ok(())
        }
    }
}

impl fmt::Debug for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JArray(rank={}, shape={:?}, type={:?})", self.rank, self.shape, self.typ)
    }
}
