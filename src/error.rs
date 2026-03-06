/// 소스 상의 위치 정보
/// Lexer가 토큰을 만들 때 동시에 계산
/// 에러 출력 시 sources[source_id][start..end] 로 원본 문자열 복원
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub source_id: usize,  // Interpreter.sources 의 인덱스
    pub start:     usize,  // 바이트 오프셋 (소스 문자열 내)
    pub end:       usize,  // 바이트 오프셋 (exclusive)
    pub line:      usize,  // 줄 번호 (1-based)
    pub col:       usize,  // 칸 번호 (1-based)
}

impl Span {
    pub fn new(source_id: usize, start: usize, end: usize, line: usize, col: usize) -> Self {
        Span { source_id, start, end, line, col }
    }

    /// 두 span을 합쳐서 더 넓은 span 생성
    /// 같은 source_id를 가정 (fork의 f~h 전체 범위 등)
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            source_id: self.source_id,
            start:     self.start.min(other.start),
            end:       self.end.max(other.end),
            line:      self.line.min(other.line),
            col:       self.col.min(other.col),
        }
    }
}

/// J 에러 종류
/// J 소스의 EVDOMAIN, EVRANK, EVLENGTH 등에 해당
#[derive(Debug, Clone, PartialEq)]
pub enum JErrorKind {
    Domain,    // EVDOMAIN: 타입/값 불일치  (+  'a')
    Rank,      // EVRANK:   rank 불일치     (rank 2 동사에 rank 3 배열)
    Length,    // EVLENGTH: 길이 불일치     (1 2 3 + 1 2)
    Index,     // EVINDEX:  범위 초과       (5 { 1 2 3)
    Value,     // EVVALUE:  미정의 이름     (undefined_name)
    Syntax,    // EVSYNTAX: 파싱 에러       (=  대신  =:)
}

impl JErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            JErrorKind::Domain  => "domain error",
            JErrorKind::Rank    => "rank error",
            JErrorKind::Length  => "length error",
            JErrorKind::Index   => "index error",
            JErrorKind::Value   => "value error",
            JErrorKind::Syntax  => "syntax error",
        }
    }
}

/// J 에러 타입
/// kind + span + message 세 가지를 함께 보관
#[derive(Debug, Clone)]
pub struct JError {
    pub kind:    JErrorKind,
    pub span:    Option<Span>,   // 에러가 발생한 토큰 위치
    pub message: String,         // 상세 메시지
}

impl JError {
    pub fn new(kind: JErrorKind, span: Option<Span>, message: impl Into<String>) -> Self {
        JError { kind, span, message: message.into() }
    }

    /// span 없는 에러 (내부 로직 오류 등)
    pub fn no_loc(kind: JErrorKind, message: impl Into<String>) -> Self {
        JError { kind, span: None, message: message.into() }
    }

    /// 에러 출력 - Python 스타일 위치 마킹
    ///
    /// line 3: mean =: +/ % 0
    ///                      ~
    /// domain error: divide by zero
    ///
    /// sources: Interpreter.sources 전체를 전달
    /// span.source_id 로 해당 소스를 찾고
    /// span.start..end 로 원본 토큰 문자열을 잘라냄
    pub fn display(&self, sources: &[String]) {
        if let Some(span) = &self.span {
            // sources[source_id] 에서 해당 줄 추출
            if let Some(source) = sources.get(span.source_id) {
                let line_str = source.lines()
                    .nth(span.line - 1)
                    .unwrap_or("");

                println!("line {}: {}", span.line, line_str);

                // col 위치에 ~~~ 마킹
                // "line N: " 의 길이만큼 들여쓰기
                let prefix_len = format!("line {}: ", span.line).len();
                let indent  = " ".repeat(prefix_len + span.col - 1);

                // span.start..end 로 원본 토큰 길이 계산
                // 단, 한 줄을 넘지 않도록 clamp
                let marker_len = (span.end - span.start).max(1);
                let marker  = "~".repeat(marker_len);
                println!("{}{}", indent, marker);
            }
        }
        println!("{}: {}", self.kind.as_str(), self.message);
    }
}

/// 편의를 위한 Result 타입 별칭
pub type JResult<T> = Result<T, JError>;
