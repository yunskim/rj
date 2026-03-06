use std::fmt;
use std::sync::Arc;

/// J의 AT 필드에 해당
#[derive(Debug, Clone, PartialEq)]
pub enum JType {
    Integer,
    Float,
    Verb,
    Adverb,
}

/// Verb trait을 Arc로 감싼 타입
pub type VerbBox = Arc<dyn crate::verbs::Verb>;

/// J의 A 블록에 해당
/// 명사와 동사 모두 JArray로 표현
/// 데이터는 항상 flat Vec (J와 동일)
/// rank / shape 로 다차원 표현
#[derive(Clone)]
pub struct JArray {
    pub typ:   JType,
    pub rank:  usize,        // AR: 차원 수
    pub shape: Vec<usize>,   // AS: 각 차원 크기  e.g. [2,3] for 2x3
    pub count: usize,        // AN: 전체 원소 수 = shape의 곱
    pub data:  JData,        // 항상 1차원 flat Vec
}

#[derive(Clone)]
pub enum JData {
    Integer(Vec<i64>),
    Float(Vec<f64>),
    Verb(VerbBox),
}

/// J의 A 타입에 해당 - Arc로 usecount 자동 관리
pub type JVal = Arc<JArray>;

impl JArray {
    // ─────────────────────────────────────────
    // 생성자
    // ─────────────────────────────────────────

    /// 정수 스칼라 (rank 0)
    pub fn scalar_int(n: i64) -> JVal {
        Arc::new(JArray {
            typ:   JType::Integer,
            rank:  0,
            shape: vec![],
            count: 1,
            data:  JData::Integer(vec![n]),
        })
    }

    /// 정수 벡터 (rank 1)
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

    /// 임의 rank 정수 배열
    /// shape: 각 차원 크기  e.g. vec![2,3] for 2x3
    /// data: flat Vec, 길이 = shape의 곱
    ///
    /// i. 2 3  →  array_int(vec![2,3], vec![0,1,2,3,4,5])
    pub fn array_int(shape: Vec<usize>, data: Vec<i64>) -> JVal {
        let count = shape.iter().product::<usize>().max(1);
        let rank = shape.len();
        assert_eq!(data.len(), count,
            "data length {} != shape product {}", data.len(), count);
        Arc::new(JArray {
            typ:   JType::Integer,
            rank,
            shape,
            count,
            data:  JData::Integer(data),
        })
    }

    /// 동사를 JArray로 감싸기
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
        match &self.data {
            JData::Integer(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_verb(&self) -> Option<&VerbBox> {
        match &self.data {
            JData::Verb(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_verb(&self) -> bool {
        self.typ == JType::Verb
    }

    pub fn is_scalar(&self) -> bool {
        self.rank == 0
    }

    // ─────────────────────────────────────────
    // rank / shape / index 헬퍼
    // ─────────────────────────────────────────

    /// 다차원 인덱스 → flat 인덱스
    /// indices: [i, j, k, ...]  각 차원의 인덱스
    ///
    /// 예: shape=[2,3], indices=[1,2] → 1*3 + 2 = 5
    ///
    /// J의 AS/AN 기반 인덱스 계산과 동일
    pub fn flat_index(&self, indices: &[usize]) -> usize {
        assert_eq!(indices.len(), self.rank,
            "indices rank {} != array rank {}", indices.len(), self.rank);
        indices.iter().zip(self.shape.iter())
            .fold(0, |acc, (&idx, &dim)| acc * dim + idx)
    }

    /// flat 인덱스 → 다차원 인덱스
    /// flat_index의 역연산
    ///
    /// 예: shape=[2,3], flat=5 → [1,2]
    pub fn multi_index(&self, mut flat: usize) -> Vec<usize> {
        let mut indices = vec![0usize; self.rank];
        for i in (0..self.rank).rev() {
            indices[i] = flat % self.shape[i];
            flat /= self.shape[i];
        }
        indices
    }

    /// 특정 축(axis)을 따라 슬라이스
    /// J의 cell 개념에 해당
    ///
    /// rank 2 배열에서 row 0:
    ///   cells(0) → shape=[3], data=row 0의 데이터
    pub fn cell(&self, axis: usize, idx: usize) -> JVal {
        assert!(axis < self.rank, "axis {} >= rank {}", axis, self.rank);
        match &self.data {
            JData::Integer(v) => {
                // axis=0 이면 idx번째 행
                let cell_size: usize = self.shape[axis+1..].iter().product::<usize>().max(1);
                let start = idx * cell_size;
                let slice = v[start..start+cell_size].to_vec();
                let new_shape = self.shape[axis+1..].to_vec();
                if new_shape.is_empty() {
                    JArray::scalar_int(slice[0])
                } else {
                    JArray::array_int(new_shape, slice)
                }
            }
            _ => panic!("cell not supported for non-integer arrays"),
        }
    }

    /// 현재 배열의 leading axis 크기
    /// rank 0: 1, rank 1+: shape[0]
    /// J의 # (tally) 와 동일
    pub fn tally(&self) -> usize {
        if self.rank == 0 { 1 } else { self.shape[0] }
    }
}

// ─────────────────────────────────────────
// Display - J의 출력 형식과 유사하게
// ─────────────────────────────────────────

impl fmt::Display for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            JData::Integer(v) => fmt_int_array(f, v, &self.shape, self.rank),
            JData::Float(v)   => fmt_float_array(f, v, &self.shape, self.rank),
            JData::Verb(v)    => write!(f, "(verb:{})", v.name()),
        }
    }
}

/// 정수 배열 출력
/// rank 0: 단일 숫자
/// rank 1: 공백 구분 숫자
/// rank 2: 줄바꿈으로 구분된 행
/// rank 3+: 빈 줄로 구분된 행렬
fn fmt_int_array(f: &mut fmt::Formatter<'_>, v: &[i64],
                 shape: &[usize], rank: usize) -> fmt::Result {
    match rank {
        0 => write!(f, "{}", v[0]),
        1 => {
            let s: Vec<String> = v.iter().map(|x| x.to_string()).collect();
            write!(f, "{}", s.join(" "))
        }
        2 => {
            let cols = shape[1];
            for (i, row) in v.chunks(cols).enumerate() {
                if i > 0 { writeln!(f)?; }
                let s: Vec<String> = row.iter().map(|x| x.to_string()).collect();
                write!(f, "{}", s.join(" "))?;
            }
            Ok(())
        }
        _ => {
            // rank 3+: 각 행렬을 빈 줄로 구분
            let matrix_size: usize = shape[rank-2..].iter().product();
            for (i, matrix) in v.chunks(matrix_size).enumerate() {
                if i > 0 { writeln!(f)?; writeln!(f)?; }
                let cols = shape[rank-1];
                for (j, row) in matrix.chunks(cols).enumerate() {
                    if j > 0 { writeln!(f)?; }
                    let s: Vec<String> = row.iter().map(|x| x.to_string()).collect();
                    write!(f, "{}", s.join(" "))?;
                }
            }
            Ok(())
        }
    }
}

fn fmt_float_array(f: &mut fmt::Formatter<'_>, v: &[f64],
                   shape: &[usize], rank: usize) -> fmt::Result {
    match rank {
        0 => write!(f, "{}", v[0]),
        1 => {
            let s: Vec<String> = v.iter().map(|x| x.to_string()).collect();
            write!(f, "{}", s.join(" "))
        }
        _ => {
            let cols = shape[rank-1];
            for (i, row) in v.chunks(cols).enumerate() {
                if i > 0 { writeln!(f)?; }
                let s: Vec<String> = row.iter().map(|x| x.to_string()).collect();
                write!(f, "{}", s.join(" "))?;
            }
            Ok(())
        }
    }
}

impl fmt::Debug for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JArray(rank={}, shape={:?})", self.rank, self.shape)
    }
}
