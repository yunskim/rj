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
fn eval_rtl(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }

    // 오른쪽 끝이 동사 토큰이면 순수 동사 표현식
    let last = &tokens[tokens.len() - 1];
    if !matches!(last, Token::Number(_) | Token::Name(_)) {
        let vb = parse_verb_expr(interp, tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    // 오른쪽 끝에서부터 연속된 명사 토큰 범위 찾기
    // i. 2 3  → verb=[i.], nouns=[2, 3]
    // +/ a    → verb=[+,/], nouns=[a]
    // 2 3 4   → verb=[], nouns=[2, 3, 4]
    let noun_start = find_noun_start(tokens);

    let noun_tokens = &tokens[noun_start..];
    let w = eval_noun_list(interp, noun_tokens)?;

    // 명사만 있는 경우 바로 반환
    if noun_start == 0 {
        return Ok(w);
    }

    let verb_tokens = &tokens[..noun_start];

    // w가 동사이면 tacit 합성
    if w.is_verb() {
        let vb = parse_verb_expr(interp, verb_tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    // 동사를 w에 적용
    let verb = parse_verb_expr(interp, verb_tokens)?;
    verb.monad(interp, &w)
}

/// 오른쪽 끝에서부터 연속된 명사 토큰의 시작 인덱스
///
/// [+, /, i., 2, 3]  → 3
/// [+, /, a]         → 2
/// [2, 3, 4]         → 0
fn find_noun_start(tokens: &[Token]) -> usize {
    let mut i = tokens.len();
    while i > 0 {
        match &tokens[i - 1] {
            Token::Number(_) | Token::Name(_) => i -= 1,
            _ => break,
        }
    }
    i
}

/// 명사 토큰 목록 → JVal
///
/// "10"     → scalar_int(10)
/// "2 3 4"  → vector_int([2,3,4])   ← i. 2 3 의 인자가 됨
/// "a"      → SymTable 조회
fn eval_noun_list(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {
    if tokens.is_empty() {
        return Err("empty noun".to_string());
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0] {
            Token::Number(n) => Ok(JArray::scalar_int(*n)),
            Token::Name(name) => {
                interp.lookup(name)
                    .ok_or_else(|| format!("undefined name: '{}'", name))
            }
            Token::Verb(v) => {
                let vb = make_primitive(v)?;
                Ok(JArray::from_verb(vb))
            }
            _ => Err("unexpected token".to_string()),
        };
    }

    // 복수 숫자 토큰 → 정수 벡터
    // "2 3 4" → vector_int([2,3,4])
    // 이것이 i. 2 3 의 w 인자가 됨
    let mut nums = Vec::new();
    for tok in tokens {
        match tok {
            Token::Number(n) => nums.push(*n),
            Token::Name(name) => {
                if nums.is_empty() {
                    return interp.lookup(name)
                        .ok_or_else(|| format!("undefined name: '{}'", name));
                }
                return Err(format!("unexpected name in noun list: '{}'", name));
            }
            _ => return Err(format!("unexpected token in noun list: {:?}", tok)),
        }
    }
    Ok(JArray::vector_int(nums))
}

/// 동사 토큰 목록 → VerbBox
///
/// "+"       → Plus
/// "i."      → Iota
/// "+/"      → Slash { Plus }
/// "+/ % #"  → Fork { Slash(Plus), Percent, Hash }
/// "mean"    → SymTable에서 동사 조회
fn parse_verb_expr(interp: &Interpreter, tokens: &[Token]) -> Result<VerbBox, String> {
    if tokens.is_empty() {
        return Err("expected verb expression".to_string());
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0] {
            Token::Verb(v) => make_primitive(v),
            Token::Name(name) => {
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

    // fork: f g h  (동사 단위 3개)
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

    Err(format!("cannot parse verb expression: {:?}", tokens))
}

/// 토큰 목록을 동사 단위로 분할
/// +/ % #  → [[+,/], [%], [#]]
fn split_into_verb_units(tokens: &[Token]) -> Result<Vec<&[Token]>, String> {
    let mut units: Vec<&[Token]> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            Token::Verb(_) => {
                // "verb adverb" 는 한 단위
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

/// VerbBox 래퍼
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
