/// 소스 상의 위치 정보
/// Lexer가 토큰을 만들 때 동시에 계산
/// 에러 출력 시 해당 위치에 ~~~ 마킹
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,   // 바이트 오프셋 (소스 문자열 내)
    pub end:   usize,   // 바이트 오프셋 (exclusive)
    pub line:  usize,   // 줄 번호 (1-based)
    pub col:   usize,   // 칸 번호 (1-based)
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Span { start, end, line, col }
    }

    /// 두 span을 합쳐서 더 넓은 span 생성
    /// 예: fork의 f~h 전체 범위
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end:   self.end.max(other.end),
            line:  self.line.min(other.line),
            col:   self.col.min(other.col),
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
    pub fn display(&self, source: &str) {
        if let Some(span) = &self.span {
            // 해당 줄 추출
            let line_str = source.lines()
                .nth(span.line - 1)
                .unwrap_or("");

            println!("line {}: {}", span.line, line_str);

            // col 위치에 ~~~ 마킹
            // "line N: " 의 길이만큼 들여쓰기
            let prefix_len = format!("line {}: ", span.line).len();
            let indent  = " ".repeat(prefix_len + span.col - 1);
            let marker_len = (span.end - span.start).max(1);
            let marker  = "~".repeat(marker_len);
            println!("{}{}", indent, marker);
        }
        println!("{}: {}", self.kind.as_str(), self.message);
    }
}

/// 편의를 위한 Result 타입 별칭
pub type JResult<T> = Result<T, JError>;
