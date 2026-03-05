use crate::array::{JArray, JVal};
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

            // 숫자
            '0'..='9' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(num.parse().unwrap()));
            }

            // =:
            '=' => {
                chars.next();
                if chars.peek() == Some(&':') {
                    chars.next();
                    tokens.push(Token::Assign);
                } else {
                    return Err("unexpected '='".to_string());
                }
            }

            // / (adverb)
            '/' => {
                chars.next();
                tokens.push(Token::Adverb("/".to_string()));
            }

            // + (verb)
            '+' => {
                chars.next();
                tokens.push(Token::Verb("+".to_string()));
            }

            // 알파벳으로 시작: 이름 또는 i. 같은 동사
            'a'..='z' | 'A'..='Z' => {
                let mut word = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' {
                        word.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                // i. 처럼 점이 붙는 동사 확인
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

/// 동사 이름으로 Verb 객체 생성
fn make_verb(name: &str) -> Result<Box<dyn Verb>, String> {
    match name {
        "+"  => Ok(Box::new(Plus)),
        "i." => Ok(Box::new(Iota)),
        _ => Err(format!("unknown verb: {}", name)),
    }
}

/// 평가기: 토큰 목록 → JVal
/// J의 오른쪽에서 왼쪽 평가 구현
pub fn eval(interp: &Interpreter, tokens: &[Token]) -> Result<JVal, String> {

    // =: 처리 (이름 바인딩)
    // "name =: expr" 형태
    if tokens.len() >= 3 {
        if let Token::Name(name) = &tokens[0] {
            if let Token::Assign = &tokens[1] {
                let val = eval(interp, &tokens[2..])?;
                interp.assign_global(name.clone(), Arc::clone(&val));
                return Ok(val);
            }
        }
    }

    // 오른쪽에서 왼쪽으로 평가
    // tokens를 오른쪽부터 처리
    eval_rtl(interp, tokens)
}

/// 오른쪽에서 왼쪽 평가
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
            _ => Err("unexpected token".to_string()),
        };
    }

    // 오른쪽 끝부터 명사(값)를 찾고
    // 그 앞의 동사를 적용
    // 예: +/ i. 10
    //     ↑  ↑  ↑
    //     |  |  명사: 10
    //     |  동사: i.  → monad i. 10 = 0..9
    //     동사+부사: +/ → monad +/ (0..9) = 45

    // 가장 오른쪽 명사 평가
    let last = &tokens[tokens.len() - 1];
    let w = match last {
        Token::Number(n) => Ok(JArray::scalar_int(*n)),
        Token::Name(name) => {
            interp.lookup(name)
                .ok_or_else(|| format!("undefined name: {}", name))
        }
        _ => Err("expected noun on right".to_string()),
    }?;

    if tokens.len() == 1 {
        return Ok(w);
    }

    // 나머지 토큰으로 동사 구성
    // 예: ["+", "/"] → Slash(Plus)
    //     ["i."]     → Iota
    let verb = make_verb_from_tokens(&tokens[..tokens.len() - 1])?;

    verb.monad(interp, &w)
}

/// 토큰 목록에서 동사 구성
/// "+/" → Slash { u: Plus }
/// "i." → Iota
fn make_verb_from_tokens(tokens: &[Token]) -> Result<Box<dyn Verb>, String> {
    if tokens.is_empty() {
        return Err("expected verb".to_string());
    }

    // 부사가 있는 경우: verb adverb 순서
    // "+/" = [Verb("+"), Adverb("/")]
    if tokens.len() == 2 {
        if let (Token::Verb(v), Token::Adverb(adv)) = (&tokens[0], &tokens[1]) {
            let u = make_verb(v)?;
            match adv.as_str() {
                "/" => return Ok(Box::new(Slash { u })),
                _ => return Err(format!("unknown adverb: {}", adv)),
            }
        }
    }

    // 단순 동사
    if tokens.len() == 1 {
        if let Token::Verb(v) = &tokens[0] {
            return make_verb(v);
        }
    }

    Err("cannot parse verb".to_string())
}
