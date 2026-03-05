use crate::array::{JArray, JVal, VerbBox};
use crate::interp::Interpreter;
use crate::verbs::{Iota, Plus, Slash, Verb};
use std::sync::Arc;

/// 토큰 타입
#[derive(Debug, Clone)]
pub enum Token {
    Number(i64),         // 숫자 리터럴
    Name(String),        // 이름 (변수)
    Verb(String),        // 동사: +, i. 등
    Adverb(String),      // 부사: /
    Assign,              // =:
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

            'a'..='z' | 'A'..='Z' => {
                let mut word = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' { word.push(d); chars.next(); }
                    else { break; }
                }
                if chars.peek() == Some(&'.') {
                    word.push('.');
                    chars.next();
                    tokens.push(Token::Verb(word));
                } else {
                    tokens.push(Token::Name(word));
                }
            }

            _ => return Err(format!("unexpected character: {}", c)),
        }
    }

    Ok(tokens)
}

/// 동사 이름으로 VerbBox 생성
fn make_verbbox(name: &str) -> Result<VerbBox, String> {
    match name {
        "+"  => Ok(Arc::new(Plus)),
        "i." => Ok(Arc::new(Iota)),
        _ => Err(format!("unknown verb: {}", name)),
    }
}

/// 평가기: 토큰 목록 → JVal
/// 동사도 JVal(JArray)로 반환
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
/// 동사 표현식도 JArray::from_verb()로 JVal 반환
fn eval_rtl(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0] {
            Token::Number(n) => Ok(JArray::scalar_int(*n)),
            Token::Name(name) => {
                interp.lookup(name)
                    .ok_or_else(|| format!("undefined name: {}", name))
            }
            // 단독 동사 토큰 → JArray로 감싸서 반환
            // mean =: +/ 에서 +/ 가 JVal이 되는 것과 같은 원리
            Token::Verb(v) => {
                let vb = make_verbbox(v)?;
                Ok(JArray::from_verb(vb))
            }
            _ => Err("unexpected token".to_string()),
        };
    }

    // 오른쪽 끝 토큰 평가
    let last = &tokens[tokens.len() - 1];
    let w: JVal = match last {
        Token::Number(n) => JArray::scalar_int(*n),
        Token::Name(name) => {
            interp.lookup(name)
                .ok_or_else(|| format!("undefined name: {}", name))?
        }
        _ => return Err("expected noun on right".to_string()),
    };

    // 왼쪽 토큰들로 동사 구성 후 적용
    let verb_tokens = &tokens[..tokens.len() - 1];
    let verb = make_verb_from_tokens(verb_tokens)?;

    // w가 동사면 tacit: 동사 합성 결과를 JArray로 반환
    // w가 명사면 일반 적용
    if w.is_verb() {
        // 예: +/ 에서 / 가 adverb로 + 에 적용된 경우
        // 현재는 단순히 verb를 JArray로 감싸서 반환
        let combined = Arc::new(Slash {
            u: Arc::clone(w.as_verb().unwrap()),
        }) as VerbBox;
        Ok(JArray::from_verb(combined))
    } else {
        verb.monad(interp, &w)
    }
}

/// 토큰 목록에서 동사 구성
/// "+/" → Slash { u: Plus }
/// "i." → Iota
/// 결과를 Box<dyn Verb>로 반환
fn make_verb_from_tokens(tokens: &[Token]) -> Result<Box<dyn Verb>, String> {
    if tokens.is_empty() {
        return Err("expected verb".to_string());
    }

    // "verb adverb" 형태: "+/"
    if tokens.len() == 2 {
        if let (Token::Verb(v), Token::Adverb(adv)) = (&tokens[0], &tokens[1]) {
            let u = make_verbbox(v)?;
            return match adv.as_str() {
                "/" => Ok(Box::new(Slash { u })),
                _   => Err(format!("unknown adverb: {}", adv)),
            };
        }
    }

    // 단순 동사
    if tokens.len() == 1 {
        if let Token::Verb(v) = &tokens[0] {
            let vb = make_verbbox(v)?;
            // VerbBox → Box<dyn Verb>로 변환
            return Ok(Box::new(VerbWrapper(vb)));
        }
        // 이름으로 바인딩된 동사 처리
        // mean =: +/ 후 mean i. 10 같은 경우
        if let Token::Name(name) = &tokens[0] {
            return Ok(Box::new(NamedVerb(name.clone())));
        }
    }

    Err("cannot parse verb".to_string())
}

/// VerbBox를 Box<dyn Verb>처럼 쓰기 위한 래퍼
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

/// 심볼 테이블에서 동사를 조회하는 래퍼
/// mean =: +/  후  mean i. 10  같은 tacit 사용을 위해
struct NamedVerb(String);

impl Verb for NamedVerb {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> Result<JVal, String> {
        let val = interp.lookup(&self.0)
            .ok_or_else(|| format!("undefined name: {}", self.0))?;
        if let Some(verb) = val.as_verb() {
            verb.monad(interp, w)
        } else {
            Err(format!("{} is not a verb", self.0))
        }
    }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> Result<JVal, String> {
        let val = interp.lookup(&self.0)
            .ok_or_else(|| format!("undefined name: {}", self.0))?;
        if let Some(verb) = val.as_verb() {
            verb.dyad(interp, a, w)
        } else {
            Err(format!("{} is not a verb", self.0))
        }
    }
    fn name(&self) -> &str { &self.0 }
}

