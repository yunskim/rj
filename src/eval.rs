use crate::array::{JArray, JVal, VerbBox};
use crate::interp::Interpreter;
use crate::verbs::{Fork, Hash, Iota, Percent, Plus, Slash, Verb};
use std::sync::Arc;

/// 토큰 타입
#[derive(Debug, Clone)]
pub enum Token {
    Number(i64),      // 숫자 리터럴
    Name(String),     // 이름 (변수 또는 동사 이름)
    Verb(String),     // primitive 동사: +, %, #, i. 등
    Adverb(String),   // 부사: /
    Assign,           // =:
}

/// Lexer: 문자열 → 토큰 목록
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.trim().chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => { chars.next(); }

            '0'..='9' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() { num.push(d); chars.next(); }
                    else { break; }
                }
                tokens.push(Token::Number(num.parse().unwrap()));
            }

            '=' => {
                chars.next();
                if chars.peek() == Some(&':') {
                    chars.next();
                    tokens.push(Token::Assign);
                } else {
                    return Err("unexpected '='".to_string());
                }
            }

            '/' => { chars.next(); tokens.push(Token::Adverb("/".to_string())); }
            '+' => { chars.next(); tokens.push(Token::Verb("+".to_string())); }
            '%' => { chars.next(); tokens.push(Token::Verb("%".to_string())); }
            '#' => { chars.next(); tokens.push(Token::Verb("#".to_string())); }

            'a'..='z' | 'A'..='Z' => {
                let mut word = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' { word.push(d); chars.next(); }
                    else { break; }
                }
                // i. 처럼 점이 붙는 primitive 동사
                if chars.peek() == Some(&'.') {
                    word.push('.');
                    chars.next();
                    tokens.push(Token::Verb(word));
                } else {
                    tokens.push(Token::Name(word));
                }
            }

            _ => return Err(format!("unexpected character: '{}'", c)),
        }
    }

    Ok(tokens)
}

/// primitive 동사 이름 → VerbBox
fn make_primitive(name: &str) -> Result<VerbBox, String> {
    match name {
        "+"  => Ok(Arc::new(Plus)),
        "%"  => Ok(Arc::new(Percent)),
        "#"  => Ok(Arc::new(Hash)),
        "i." => Ok(Arc::new(Iota)),
        _ => Err(format!("unknown verb: {}", name)),
    }
}

/// 평가기 진입점
/// 명사 → JVal (데이터)
/// 동사 표현식 → JVal (JArray::from_verb로 감싼 동사)
pub fn eval(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {
    // =: 처리: "name =: expr"
    if tokens.len() >= 3 {
        if let Token::Name(name) = &tokens[0] {
            if let Token::Assign = &tokens[1] {
                let val = eval(interp, &tokens[2..])?;
                interp.assign_global(name.clone(), Arc::clone(&val));
                return Ok(val);
            }
        }
    }

    eval_rtl(interp, tokens)
}

/// 오른쪽에서 왼쪽 평가
///
/// J의 파싱 규칙:
///   토큰을 오른쪽부터 보면서
///   명사가 나오면 스택에 쌓고
///   동사가 나오면 명사에 적용
///
/// 동사 표현식만 있으면 (명사 없으면) derived verb 반환
fn eval_rtl(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }

    // 토큰 목록을 "동사 토큰 그룹"으로 분류
    // 오른쪽 끝이 명사인지 동사인지 확인
    let last = &tokens[tokens.len() - 1];

    let is_noun = matches!(last, Token::Number(_) | Token::Name(_));

    if !is_noun {
        // 오른쪽 끝이 동사 → 순수 동사 표현식 → VerbBox로 반환
        // 예: +/ % #  → Fork { Slash(Plus), Percent, Hash }
        // 예: +/      → Slash { Plus }
        let vb = parse_verb_expr(interp, tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    // 오른쪽 끝이 명사 → w 계산 후 동사 적용
    let w = eval_noun(interp, last)?;

    if tokens.len() == 1 {
        return Ok(w);
    }

    // 나머지 왼쪽 토큰들 → 동사
    let verb_tokens = &tokens[..tokens.len() - 1];

    // w가 동사이면 (심볼 테이블에서 꺼낸 derived verb)
    // verb_tokens + w 전체가 동사 합성
    if w.is_verb() {
        // 예: sum =: +/  이후  sum % #  같은 경우
        // → Fork 구성
        let left_vb = parse_verb_expr(interp, verb_tokens)?;
        let right_vb = Arc::clone(w.as_verb().unwrap());
        // verb_tokens가 단일 동사이면 그냥 반환, 아니면 fork
        // 실제로는 이 경우가 아직 없으므로 단순 처리
        let _ = right_vb;
        return Ok(JArray::from_verb(left_vb));
    }

    // 일반 경우: 동사를 w에 적용
    let verb = parse_verb_expr(interp, verb_tokens)?;
    verb.monad(interp, &w)
}

/// 명사 토큰 → JVal
fn eval_noun(interp: &Interpreter, token: &Token) -> Result<JVal, String> {
    match token {
        Token::Number(n) => Ok(JArray::scalar_int(*n)),
        Token::Name(name) => {
            interp.lookup(name)
                .ok_or_else(|| format!("undefined name: '{}'", name))
        }
        _ => Err("expected noun".to_string()),
    }
}

/// 동사 토큰 목록 → VerbBox
///
/// 지원하는 패턴:
///   v           → primitive: +, %, #, i.
///   name        → named verb (심볼 테이블 조회)
///   v adv       → derived: +/
///   f g h       → fork: +/ % #  = (f w) g (h w)
///   f adv g h   → fork with adverb: +/ % #
///
/// J의 동사 파싱은 오른쪽에서 왼쪽이지만
/// fork는 f g h 순서로 읽음
fn parse_verb_expr(interp: &Interpreter, tokens: &[Token]) -> Result<VerbBox, String> {
    if tokens.is_empty() {
        return Err("expected verb expression".to_string());
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0] {
            Token::Verb(v) => make_primitive(v),
            Token::Name(name) => {
                // 심볼 테이블에서 동사 조회
                let val = interp.lookup(name)
                    .ok_or_else(|| format!("undefined name: '{}'", name))?;
                val.as_verb()
                    .map(Arc::clone)
                    .ok_or_else(|| format!("'{}' is not a verb", name))
            }
            _ => Err("expected verb".to_string()),
        };
    }

    // "verb adverb" 패턴: +/
    if tokens.len() == 2 {
        if let (Token::Verb(v), Token::Adverb(adv)) = (&tokens[0], &tokens[1]) {
            let u = make_primitive(v)?;
            return match adv.as_str() {
                "/" => Ok(Arc::new(Slash { u })),
                _   => Err(format!("unknown adverb: '{}'", adv)),
            };
        }
    }

    // fork 패턴 파싱: f g h
    // 토큰들을 왼쪽부터 동사 단위로 분할
    // 예: +/ % #  → [+/] [%] [#]  → Fork { Slash(+), %, # }
    // 예: i. % #  → [i.] [%] [#]  → Fork { Iota, %, # }
    let verb_units = split_into_verb_units(tokens)?;

    if verb_units.len() == 3 {
        // fork: f g h
        let f = parse_verb_expr(interp, verb_units[0])?;
        let g = parse_verb_expr(interp, verb_units[1])?;
        let h = parse_verb_expr(interp, verb_units[2])?;
        return Ok(Arc::new(Fork { f, g, h }));
    }

    if verb_units.len() == 1 {
        return parse_verb_expr(interp, verb_units[0]);
    }

    Err(format!("cannot parse verb expression: {:?}", tokens))
}

/// 토큰 목록을 동사 단위로 분할
/// +/ % #  → [[+, /], [%], [#]]
/// i. % #  → [[i.], [%], [#]]
fn split_into_verb_units(tokens: &[Token]) -> Result<Vec<&[Token]>, String> {
    let mut units: Vec<&[Token]> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            // "verb adverb" 는 한 단위
            Token::Verb(_) => {
                if i + 1 < tokens.len() {
                    if let Token::Adverb(_) = &tokens[i + 1] {
                        units.push(&tokens[i..i+2]);
                        i += 2;
                        continue;
                    }
                }
                units.push(&tokens[i..i+1]);
                i += 1;
            }
            // 이름도 동사 단위 하나
            Token::Name(_) => {
                units.push(&tokens[i..i+1]);
                i += 1;
            }
            Token::Adverb(_) => {
                return Err("unexpected adverb".to_string());
            }
            _ => {
                return Err(format!("unexpected token in verb expression: {:?}", tokens[i]));
            }
        }
    }

    Ok(units)
}

/// VerbBox를 Box<dyn Verb>처럼 쓰기 위한 래퍼
/// make_verb_from_tokens의 반환 타입 통일용
struct VerbWrapper(VerbBox);

impl Verb for VerbWrapper {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        self.0.monad(interp, w)
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String> {
        self.0.dyad(interp, a, w)
    }
    fn name(&self) -> &str { self.0.name() }
}
