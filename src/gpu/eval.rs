use crate::array::{JArray, JVal, VerbBox};
#[allow(unused_imports)]
use crate::gpu::Backend;
use crate::error::{JError, JErrorKind, JResult, Span};
use crate::interp::Interpreter;
use crate::verbs::{rank1ex, rank2ex, Bar, Dollar, Eq, Fork, Ge, Gt, Hash, Iota, Le, Lt, Minus, Ne, Percent, Plus, Slash, Star, Tco, Verb};
use std::sync::Arc;

// ─────────────────────────────────────────
// Token
// ─────────────────────────────────────────

/// 토큰 종류
#[derive(Debug, Clone)]
pub enum TokenKind {
    Number(i64),        // 정수 리터럴: 3
    Float(f64),         // 실수 리터럴: 3.14
    Complex(f64, f64),  // 복소수 리터럴: 3j4
    Name(String),
    Verb(String),
    Adverb(String),
    Conjunction(String), // t. 등: "verb conjunction" → 새 동사
    Assign,
}

/// 위치 정보를 포함한 토큰
/// Span을 처음부터 포함해야 나중에 에러 위치 추적 가능
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }
}

// ─────────────────────────────────────────
// Lexer
// ─────────────────────────────────────────

/// Lexer: 문자열 → 토큰 목록
/// source_id: Interpreter.sources 에서의 인덱스
/// 각 토큰의 Span에 source_id를 포함시켜
/// 나중에 sources[source_id][span.start..span.end] 로 원본 복원 가능
pub fn tokenize(input: &str, source_id: usize) -> JResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars  = input.chars().peekable();

    let mut pos  = 0usize;
    let mut line = 1usize;
    let mut col  = 1usize;

    macro_rules! advance {
        () => {{
            let c = chars.next().unwrap();
            let sp = pos; let sl = line; let sc = col;
            if c == '\n' { line += 1; col = 1; }
            else { col += c.len_utf8(); }
            pos += c.len_utf8();
            (c, sp, sl, sc)
        }};
    }

    // Span 생성 헬퍼 - source_id 자동 포함
    macro_rules! span {
        ($start:expr, $sl:expr, $sc:expr) => {
            Span::new(source_id, $start, pos, $sl, $sc)
        };
    }

    while let Some(&c) = chars.peek() {
        match c {
            // 공백 건너뜀
            ' ' | '\t' | '\n' => { advance!(); }

            // 숫자 리터럴: 정수 / 실수 / 복소수
            // 3       → Integer
            // 3.14    → Float
            // 3j4     → Complex (J 표기)
            // 3.5j2.1 → Complex
            '0'..='9' => {
                let (_, start, sl, sc) = advance!();
                let mut num = String::from(c);

                // 정수 또는 소수점 앞부분 수집
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        let (dc, _, _, _) = advance!();
                        num.push(dc);
                    } else { break; }
                }

                // 소수점?
                let is_float = chars.peek() == Some(&'.');
                if is_float {
                    advance!();  // '.' 소비
                    num.push('.');
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            let (dc, _, _, _) = advance!();
                            num.push(dc);
                        } else { break; }
                    }
                }

                // j 로 복소수?
                if chars.peek() == Some(&'j') {
                    advance!();  // 'j' 소비
                    let mut imag = String::new();
                    // 허수부 수집 (소수점 포함 가능)
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() || d == '.' {
                            let (dc, _, _, _) = advance!();
                            imag.push(dc);
                        } else { break; }
                    }
                    let r: f64 = num.parse().map_err(|_|
                        JError::new(JErrorKind::Syntax, Some(span!(start, sl, sc)),
                            format!("invalid complex real: {}", num)))?;
                    let i: f64 = imag.parse().map_err(|_|
                        JError::new(JErrorKind::Syntax, Some(span!(start, sl, sc)),
                            format!("invalid complex imag: {}", imag)))?;
                    tokens.push(Token::new(TokenKind::Complex(r, i), span!(start, sl, sc)));
                } else if is_float {
                    let x: f64 = num.parse().map_err(|_|
                        JError::new(JErrorKind::Syntax, Some(span!(start, sl, sc)),
                            format!("invalid float: {}", num)))?;
                    tokens.push(Token::new(TokenKind::Float(x), span!(start, sl, sc)));
                } else {
                    let n: i64 = num.parse().map_err(|_|
                        JError::new(JErrorKind::Syntax, Some(span!(start, sl, sc)),
                            format!("invalid integer: {}", num)))?;
                    tokens.push(Token::new(TokenKind::Number(n), span!(start, sl, sc)));
                }
            }

            // =: (assign)
            '=' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') {
                    advance!();
                    tokens.push(Token::new(TokenKind::Assign, span!(start, sl, sc)));
                } else {
                    return Err(JError::new(JErrorKind::Syntax,
                        Some(span!(start, sl, sc)),
                        "expected ':' after '='"));
                }
            }

            // 단일 문자 동사/부사
            '/' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Adverb("/".into()), span!(start, sl, sc)));
            }
            '+' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("+".into()), span!(start, sl, sc)));
            }
            '%' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("%".into()), span!(start, sl, sc)));
            }
            '#' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("#".into()), span!(start, sl, sc)));
            }
            '-' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("-".into()), span!(start, sl, sc)));
            }
            '*' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("*".into()), span!(start, sl, sc)));
            }
            '|' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("|".into()), span!(start, sl, sc)));
            }
            '$' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("$".into()), span!(start, sl, sc)));
            }
            '<' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') {
                    advance!();
                    tokens.push(Token::new(TokenKind::Verb("<:".into()), span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb("<".into()), span!(start, sl, sc)));
                }
            }
            '>' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') {
                    advance!();
                    tokens.push(Token::new(TokenKind::Verb(">:".into()), span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb(">".into()), span!(start, sl, sc)));
                }
            }

            // 알파벳으로 시작: 이름 또는 i./t. 같은 동사/conjunction
            'a'..='z' | 'A'..='Z' => {
                let (_, start, sl, sc) = advance!();
                let mut word = String::from(c);
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' {
                        let (dc, _, _, _) = advance!();
                        word.push(dc);
                    } else { break; }
                }
                // i. 처럼 점이 붙는 primitive
                if chars.peek() == Some(&'.') {
                    advance!();
                    word.push('.');
                    // t. 는 conjunction (verb u t. → Tco(u))
                    // i. 등 나머지는 verb
                    if word == "t." {
                        tokens.push(Token::new(TokenKind::Conjunction(word), span!(start, sl, sc)));
                    } else {
                        tokens.push(Token::new(TokenKind::Verb(word), span!(start, sl, sc)));
                    }
                } else {
                    tokens.push(Token::new(TokenKind::Name(word), span!(start, sl, sc)));
                }
            }

            _ => {
                let (_, start, sl, sc) = advance!();
                return Err(JError::new(
                    JErrorKind::Syntax,
                    Some(span!(start, sl, sc)),
                    format!("unexpected character: '{}'", c),
                ));
            }
        }
    }

    Ok(tokens)
}

// ─────────────────────────────────────────
// 평가기
// ─────────────────────────────────────────

/// primitive 동사 이름 → VerbBox
fn make_primitive(name: &str, span: &Span) -> JResult<VerbBox> {
    match name {
        "+"  => Ok(Arc::new(Plus)),
        "-"  => Ok(Arc::new(Minus)),
        "*"  => Ok(Arc::new(Star)),
        "%"  => Ok(Arc::new(Percent)),
        "|"  => Ok(Arc::new(Bar)),
        "#"  => Ok(Arc::new(Hash)),
        "$"  => Ok(Arc::new(Dollar)),
        "<"  => Ok(Arc::new(Lt)),
        ">"  => Ok(Arc::new(Gt)),
        "<:" => Ok(Arc::new(Le)),
        ">:" => Ok(Arc::new(Ge)),
        "="  => Ok(Arc::new(Eq)),
        "~:" => Ok(Arc::new(Ne)),
        "i." => Ok(Arc::new(Iota)),
        _ => Err(JError::new(
            JErrorKind::Value,
            Some(span.clone()),
            format!("unknown verb: '{}'", name),
        )),
    }
}

/// 평가기 진입점
pub fn eval(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    // =: 처리: "name =: expr"
    if tokens.len() >= 3 {
        if let TokenKind::Name(name) = &tokens[0].kind {
            if let TokenKind::Assign = &tokens[1].kind {
                let val = eval(interp, &tokens[2..])?;
                interp.assign_global(name.clone(), Arc::clone(&val));
                return Ok(val);
            }
        }
    }

    eval_rtl(interp, tokens)
}

/// 오른쪽에서 왼쪽 평가
fn eval_rtl(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "empty expression"));
    }

    // 오른쪽 끝이 동사이면 순수 동사 표현식
    let last = &tokens[tokens.len() - 1];
    if !matches!(last.kind, TokenKind::Number(_) | TokenKind::Name(_)) {
        let vb = parse_verb_expr(interp, tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    // 오른쪽에서 연속된 명사 토큰 범위
    let noun_start = find_noun_start(tokens);
    let noun_tokens = &tokens[noun_start..];
    let w = eval_noun_list(interp, noun_tokens)?;

    // 명사만 있는 경우
    if noun_start == 0 {
        return Ok(w);
    }

    let verb_tokens = &tokens[..noun_start];

    // w가 동사이면 tacit 합성
    if w.is_verb() {
        let vb = parse_verb_expr(interp, verb_tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    // 동사를 w에 적용 - rank agreement 통해서
    // J의 DF1RANK 에 해당: 동사 rank와 배열 rank를 비교 후 분기
    let verb = parse_verb_expr(interp, verb_tokens)?;
    rank1ex(verb.as_ref(), interp, &w, verb.monad_rank())
}

/// 오른쪽 끝에서부터 연속된 명사 토큰의 시작 인덱스
fn find_noun_start(tokens: &[Token]) -> usize {
    let mut i = tokens.len();
    while i > 0 {
        match &tokens[i - 1].kind {
            TokenKind::Number(_)
            | TokenKind::Float(_)
            | TokenKind::Complex(..)
            | TokenKind::Name(_) => i -= 1,
            _ => break,
        }
    }
    i
}

/// 명사 토큰 목록 → JVal
fn eval_noun_list(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "empty noun"));
    }

    if tokens.len() == 1 {
        return match &tokens[0].kind {
            TokenKind::Number(n)     => Ok(JArray::scalar_int(*n)),
            TokenKind::Float(x)      => Ok(JArray::scalar_float(*x)),
            TokenKind::Complex(r, i) => Ok(JArray::scalar_complex(*r, *i)),
            TokenKind::Name(name) => {
                interp.lookup(name).ok_or_else(|| JError::new(
                    JErrorKind::Value,
                    Some(tokens[0].span.clone()),
                    format!("undefined name: '{}'", name),
                ))
            }
            TokenKind::Verb(v) => {
                let vb = make_primitive(v, &tokens[0].span)?;
                Ok(JArray::from_verb(vb))
            }
            _ => Err(JError::new(JErrorKind::Syntax,
                Some(tokens[0].span.clone()), "unexpected token")),
        };
    }

    // 복수 토큰 → 동일 타입 벡터
    // 타입이 섞이면 numeric tower로 승격
    // 예: 1 2.5 3 → Float 벡터
    //     1 2j3   → Complex 벡터
    let mut has_float   = false;
    let mut has_complex = false;

    for tok in tokens {
        match &tok.kind {
            TokenKind::Float(_)   => has_float = true,
            TokenKind::Complex(..) => has_complex = true,
            TokenKind::Number(_)  => {}
            TokenKind::Name(name) => {
                return interp.lookup(name).ok_or_else(|| JError::new(
                    JErrorKind::Value,
                    Some(tok.span.clone()),
                    format!("undefined name: '{}'", name),
                ));
            }
            _ => return Err(JError::new(JErrorKind::Syntax,
                Some(tok.span.clone()), "unexpected token in noun list")),
        }
    }

    if has_complex {
        // Complex 벡터: 각 원소를 (r, i) 쌍으로
        let pairs: Vec<(f64, f64)> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n)     => (*n as f64, 0.0),
            TokenKind::Float(x)      => (*x, 0.0),
            TokenKind::Complex(r, i) => (*r, *i),
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_complex(pairs))
    } else if has_float {
        // Float 벡터
        let data: Vec<f64> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n) => *n as f64,
            TokenKind::Float(x)  => *x,
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_float(data))
    } else {
        // Integer 벡터
        let data: Vec<i64> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n) => *n,
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_int(data))
    }
}

/// 동사 토큰 목록 → VerbBox
fn parse_verb_expr(interp: &Interpreter, tokens: &[Token]) -> JResult<VerbBox> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "expected verb expression"));
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0].kind {
            TokenKind::Verb(v) => make_primitive(v, &tokens[0].span),
            TokenKind::Name(name) => {
                let val = interp.lookup(name).ok_or_else(|| JError::new(
                    JErrorKind::Value,
                    Some(tokens[0].span.clone()),
                    format!("undefined name: '{}'", name),
                ))?;
                val.as_verb().map(Arc::clone).ok_or_else(|| JError::new(
                    JErrorKind::Domain,
                    Some(tokens[0].span.clone()),
                    format!("'{}' is not a verb", name),
                ))
            }
            _ => Err(JError::new(
                JErrorKind::Syntax,
                Some(tokens[0].span.clone()),
                "expected verb",
            )),
        };
    }

    // "verb adverb" 패턴: +/
    // "verb conjunction" 패턴: +t.
    if tokens.len() == 2 {
        if let (TokenKind::Verb(v), TokenKind::Adverb(adv)) =
            (&tokens[0].kind, &tokens[1].kind)
        {
            let u = make_primitive(v, &tokens[0].span)?;
            return match adv.as_str() {
                "/" => Ok(Arc::new(Slash { u })),
                _   => Err(JError::new(
                    JErrorKind::Syntax,
                    Some(tokens[1].span.clone()),
                    format!("unknown adverb: '{}'", adv),
                )),
            };
        }
        // verb t. → Tco(verb)
        if let (TokenKind::Verb(v), TokenKind::Conjunction(conj)) =
            (&tokens[0].kind, &tokens[1].kind)
        {
            let u = make_primitive(v, &tokens[0].span)?;
            return match conj.as_str() {
                "t." => Ok(Arc::new(Tco { u })),
                _    => Err(JError::new(
                    JErrorKind::Syntax,
                    Some(tokens[1].span.clone()),
                    format!("unknown conjunction: '{}'", conj),
                )),
            };
        }
    }

    // fork: f g h
    let verb_units = split_into_verb_units(tokens)?;

    if verb_units.len() == 3 {
        let f = parse_verb_expr(interp, verb_units[0])?;
        let g = parse_verb_expr(interp, verb_units[1])?;
        let h = parse_verb_expr(interp, verb_units[2])?;
        return Ok(Arc::new(Fork { f, g, h }));
    }

    if verb_units.len() == 1 {
        return parse_verb_expr(interp, verb_units[0]);
    }

    // 에러 위치: 첫 토큰 ~ 마지막 토큰
    let span = tokens[0].span.merge(&tokens[tokens.len()-1].span);
    Err(JError::new(
        JErrorKind::Syntax,
        Some(span),
        "cannot parse verb expression",
    ))
}

/// 토큰 목록을 동사 단위로 분할
fn split_into_verb_units(tokens: &[Token]) -> JResult<Vec<&[Token]>> {
    let mut units: Vec<&[Token]> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i].kind {
            TokenKind::Verb(_) => {
                // "verb adverb" 또는 "verb conjunction" 는 한 단위
                if i + 1 < tokens.len() {
                    match &tokens[i + 1].kind {
                        TokenKind::Adverb(_) | TokenKind::Conjunction(_) => {
                            units.push(&tokens[i..i+2]);
                            i += 2;
                            continue;
                        }
                        _ => {}
                    }
                }
                units.push(&tokens[i..i+1]);
                i += 1;
            }
            TokenKind::Name(_) => {
                units.push(&tokens[i..i+1]);
                i += 1;
            }
            TokenKind::Adverb(_) => {
                return Err(JError::new(
                    JErrorKind::Syntax,
                    Some(tokens[i].span.clone()),
                    "unexpected adverb",
                ));
            }
            _ => {
                return Err(JError::new(
                    JErrorKind::Syntax,
                    Some(tokens[i].span.clone()),
                    "unexpected token in verb expression",
                ));
            }
        }
    }

    Ok(units)
}

/// VerbBox 래퍼
struct VerbWrapper(VerbBox);

impl Verb for VerbWrapper {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> {
        self.0.monad(interp, w)
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> {
        self.0.dyad(interp, a, w)
    }
    fn name(&self) -> &str { self.0.name() }
}
