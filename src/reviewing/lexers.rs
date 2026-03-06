use crate::array::{JArray, JVal, VerbBox};
#[allow(unused_imports)]
use crate::gpu::Backend;
use crate::error::{JError, JErrorKind, JResult, Span};
use crate::interp::Interpreter;
use crate::verbs::{
    rank1ex,
    #[allow(unused_imports)] rank2ex,
    Bar, Dollar, Eq, Fork, Ge, Gt, Hash, Iota,
    Le, Lt, Minus, Ne, Percent, Plus, Slash, Star, Tco, Verb,
};
use std::sync::Arc;

// ─────────────────────────────────────────
// Token
// ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TokenKind {
    Number(i64),        // 정수: 3  /  음수: _3
    Float(f64),         // 실수: 3.14  /  _ = infinity
    Complex(f64, f64),  // 복소수: 3j4
    Str(String),        // 문자열: 'hello' / ''
    Name(String),       // 이름: foo
    Verb(String),       // 동사: + - * % | # $ < > = i. ~:
    Adverb(String),     // 부사: /
    Conjunction(String),// conjunction: t.  .
    LParen,             // (
    RParen,             // )
    Assign,             // =:
    Foreign(u32, u32),  // n!:m  예) 236!:32
}

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

    macro_rules! span {
        ($start:expr, $sl:expr, $sc:expr) => {
            Span::new(source_id, $start, pos, $sl, $sc)
        };
    }

    while let Some(&c) = chars.peek() {
        match c {
            // 공백
            ' ' | '\t' | '\r' | '\n' => { advance!(); }

            // 양수 숫자: 3 / 3.14 / 3j4
            '0'..='9' => {
                let (_, start, sl, sc) = advance!();
                let mut num = String::from(c);
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() { let (dc,_,_,_) = advance!(); num.push(dc); }
                    else { break; }
                }
                let is_float = chars.peek() == Some(&'.');
                if is_float {
                    advance!(); num.push('.');
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() { let (dc,_,_,_) = advance!(); num.push(dc); }
                        else { break; }
                    }
                }
                if chars.peek() == Some(&'j') {
                    advance!();
                    let mut imag = String::new();
                    if chars.peek() == Some(&'_') { advance!(); imag.push('-'); }
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() || d == '.' { let (dc,_,_,_) = advance!(); imag.push(dc); }
                        else { break; }
                    }
                    let r: f64 = num.parse().map_err(|_| JError::new(JErrorKind::Syntax,
                        Some(span!(start, sl, sc)), format!("invalid complex real: {}", num)))?;
                    let i: f64 = if imag.is_empty() { 0.0 } else {
                        imag.parse().map_err(|_| JError::new(JErrorKind::Syntax,
                            Some(span!(start, sl, sc)), "invalid imaginary part".to_string()))?
                    };
                    tokens.push(Token::new(TokenKind::Complex(r, i), span!(start, sl, sc)));
                } else if is_float {
                    let x: f64 = num.parse().map_err(|_| JError::new(JErrorKind::Syntax,
                        Some(span!(start, sl, sc)), format!("invalid float: {}", num)))?;
                    tokens.push(Token::new(TokenKind::Float(x), span!(start, sl, sc)));
                } else {
                    let n: i64 = num.parse().map_err(|_| JError::new(JErrorKind::Syntax,
                        Some(span!(start, sl, sc)), format!("invalid integer: {}", num)))?;
                    tokens.push(Token::new(TokenKind::Number(n), span!(start, sl, sc)));
                }
            }

            // _ : 음수 / infinity / 이름
            '_' => {
                let (_, start, sl, sc) = advance!();
                match chars.peek() {
                    Some(&d) if d.is_ascii_digit() => {
                        let mut num = String::from('-');
                        while let Some(&d) = chars.peek() {
                            if d.is_ascii_digit() { let (dc,_,_,_) = advance!(); num.push(dc); }
                            else { break; }
                        }
                        let is_float = chars.peek() == Some(&'.');
                        if is_float {
                            advance!(); num.push('.');
                            while let Some(&d) = chars.peek() {
                                if d.is_ascii_digit() { let (dc,_,_,_) = advance!(); num.push(dc); }
                                else { break; }
                            }
                        }
                        if chars.peek() == Some(&'j') {
                            advance!();
                            let mut imag = String::new();
                            if chars.peek() == Some(&'_') { advance!(); imag.push('-'); }
                            while let Some(&d) = chars.peek() {
                                if d.is_ascii_digit() || d == '.' { let (dc,_,_,_) = advance!(); imag.push(dc); }
                                else { break; }
                            }
                            let r: f64 = num.parse().unwrap();
                            let i: f64 = if imag.is_empty() { 0.0 } else { imag.parse().unwrap_or(0.0) };
                            tokens.push(Token::new(TokenKind::Complex(r, i), span!(start, sl, sc)));
                        } else if is_float {
                            let x: f64 = num.parse().unwrap();
                            tokens.push(Token::new(TokenKind::Float(x), span!(start, sl, sc)));
                        } else {
                            let n: i64 = num.parse().unwrap();
                            tokens.push(Token::new(TokenKind::Number(n), span!(start, sl, sc)));
                        }
                    }
                    Some(&d) if d.is_alphabetic() || d == '_' => {
                        let mut word = String::from('_');
                        while let Some(&d) = chars.peek() {
                            if d.is_alphanumeric() || d == '_' {
                                let (dc,_,_,_) = advance!(); word.push(dc);
                            } else { break; }
                        }
                        tokens.push(Token::new(TokenKind::Name(word), span!(start, sl, sc)));
                    }
                    _ => {
                        // 단독 _ = infinity
                        tokens.push(Token::new(TokenKind::Float(f64::INFINITY), span!(start, sl, sc)));
                    }
                }
            }

            // 문자열 리터럴: 'abc' / ''
            '\'' => {
                let (_, start, sl, sc) = advance!();
                let mut s = String::new();
                loop {
                    match chars.peek() {
                        None => return Err(JError::new(JErrorKind::Syntax,
                            Some(span!(start, sl, sc)), "unterminated string")),
                        Some(&'\'') => {
                            advance!();
                            if chars.peek() == Some(&'\'') {
                                advance!(); s.push('\'');
                            } else { break; }
                        }
                        Some(_) => { let (dc,_,_,_) = advance!(); s.push(dc); }
                    }
                }
                tokens.push(Token::new(TokenKind::Str(s), span!(start, sl, sc)));
            }

            // 괄호
            '(' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::LParen, span!(start, sl, sc))); }
            ')' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::RParen, span!(start, sl, sc))); }

            // = / =:
            '=' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') {
                    advance!();
                    tokens.push(Token::new(TokenKind::Assign, span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb("=".into()), span!(start, sl, sc)));
                }
            }

            // ! : foreign
            '!' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') {
                    advance!();
                    tokens.push(Token::new(TokenKind::Verb("!:".into()), span!(start, sl, sc)));
                } else {
                    return Err(JError::new(JErrorKind::Syntax,
                        Some(span!(start, sl, sc)), "expected ':' after '!'"));
                }
            }

            // . : inner product conjunction
            '.' => {
                let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Conjunction(".".into()), span!(start, sl, sc)));
            }

            // 동사/부사
            '/' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Adverb("/".into()), span!(start, sl, sc))); }
            '+' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("+".into()), span!(start, sl, sc))); }
            '%' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("%".into()), span!(start, sl, sc))); }
            '#' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("#".into()), span!(start, sl, sc))); }
            '-' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("-".into()), span!(start, sl, sc))); }
            '*' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("*".into()), span!(start, sl, sc))); }
            '|' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("|".into()), span!(start, sl, sc))); }
            '$' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb("$".into()), span!(start, sl, sc))); }
            '<' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') { advance!();
                    tokens.push(Token::new(TokenKind::Verb("<:".into()), span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb("<".into()), span!(start, sl, sc)));
                }
            }
            '>' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') { advance!();
                    tokens.push(Token::new(TokenKind::Verb(">:".into()), span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb(">".into()), span!(start, sl, sc)));
                }
            }
            '~' => {
                let (_, start, sl, sc) = advance!();
                if chars.peek() == Some(&':') { advance!();
                    tokens.push(Token::new(TokenKind::Verb("~:".into()), span!(start, sl, sc)));
                } else {
                    tokens.push(Token::new(TokenKind::Verb("~".into()), span!(start, sl, sc)));
                }
            }
            ':' => { let (_, start, sl, sc) = advance!();
                tokens.push(Token::new(TokenKind::Verb(":".into()), span!(start, sl, sc))); }

            // 알파벳: 이름 / primitive / NB. 주석
            'a'..='z' | 'A'..='Z' => {
                let (_, start, sl, sc) = advance!();
                let mut word = String::from(c);
                while let Some(&d) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' {
                        let (dc,_,_,_) = advance!(); word.push(dc);
                    } else { break; }
                }
                // NB. 주석
                if chars.peek() == Some(&'.') && word == "NB" {
                    advance!();
                    while let Some(&d) = chars.peek() {
                        if d == '\n' { break; }
                        advance!();
                    }
                    continue;
                }
                // t. i. 같은 primitive
                if chars.peek() == Some(&'.') {
                    advance!(); word.push('.');
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
                return Err(JError::new(JErrorKind::Syntax,
                    Some(span!(start, sl, sc)),
                    format!("unexpected character: '{}'", c)));
            }
        }
    }

    // Foreign 합성: Number "!:" Number → Foreign(n, m)
    Ok(merge_foreign(tokens))
}

// ─────────────────────────────────────────
// Foreign 합성
// [Number(236), Verb("!:"), Number(32)] → [Foreign(236, 32)]
// ─────────────────────────────────────────

fn merge_foreign(tokens: Vec<Token>) -> Vec<Token> {
    let mut result: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if i + 2 < tokens.len() {
            if let (TokenKind::Number(n), TokenKind::Verb(op), TokenKind::Number(m)) =
                (&tokens[i].kind, &tokens[i+1].kind, &tokens[i+2].kind)
            {
                if op == "!:" {
                    let span = tokens[i].span.merge(&tokens[i+2].span);
                    result.push(Token::new(TokenKind::Foreign(*n as u32, *m as u32), span));
                    i += 3;
                    continue;
                }
            }
        }
        result.push(tokens[i].clone());
        i += 1;
    }
    result
}

// ─────────────────────────────────────────
// primitive 동사 이름 → VerbBox
// ─────────────────────────────────────────

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
        _ => Err(JError::new(JErrorKind::Value, Some(span.clone()),
            format!("unknown verb: '{}'", name))),
    }
}

// ─────────────────────────────────────────
// 괄호 헬퍼
// ─────────────────────────────────────────

fn find_matching_paren(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in start..tokens.len() {
        match &tokens[i].kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => { depth -= 1; if depth == 0 { return Some(i); } }
            _ => {}
        }
    }
    None
}

// ─────────────────────────────────────────
// Foreign 평가
// ─────────────────────────────────────────

fn eval_foreign(_interp: &Interpreter, n: u32, m: u32, _arg: &JVal) -> JResult<JVal> {
    match n {
        // 236!: GPU precision 설정 (나중에 interp.task_config 연결)
        236 => match m {
            16 | 32 | 64 => Ok(JArray::scalar_int(m as i64)),
            _ => Err(JError::no_loc(JErrorKind::Domain,
                format!("236!:{} not supported (use 16, 32, or 64)", m))),
        },
        _ => Err(JError::no_loc(JErrorKind::Domain,
            format!("foreign {}!:{} not implemented", n, m))),
    }
}

// ─────────────────────────────────────────
// 평가기
// ─────────────────────────────────────────

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

fn eval_rtl(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "empty expression"));
    }

    // Foreign: 236!:32 ''
    if let TokenKind::Foreign(n, m) = &tokens[0].kind {
        let arg = if tokens.len() > 1 {
            eval_rtl(interp, &tokens[1..])?
        } else {
            JArray::scalar_int(0)
        };
        return eval_foreign(interp, *n, *m, &arg);
    }

    // 오른쪽 끝이 동사 → 순수 동사 표현식
    let last = &tokens[tokens.len() - 1];
    if !matches!(last.kind,
        TokenKind::Number(_) | TokenKind::Float(_) | TokenKind::Complex(..) |
        TokenKind::Name(_)   | TokenKind::Str(_)   | TokenKind::RParen)
    {
        let vb = parse_verb_expr(interp, tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    let noun_start = find_noun_start(tokens);
    let noun_tokens = &tokens[noun_start..];
    let w = eval_noun_expr(interp, noun_tokens)?;

    if noun_start == 0 {
        return Ok(w);
    }

    let verb_tokens = &tokens[..noun_start];

    if w.is_verb() {
        let vb = parse_verb_expr(interp, verb_tokens)?;
        return Ok(JArray::from_verb(vb));
    }

    let verb = parse_verb_expr(interp, verb_tokens)?;
    rank1ex(verb.as_ref(), interp, &w, verb.monad_rank())
}

fn find_noun_start(tokens: &[Token]) -> usize {
    let mut i = tokens.len();
    while i > 0 {
        match &tokens[i - 1].kind {
            TokenKind::Number(_)
            | TokenKind::Float(_)
            | TokenKind::Complex(..)
            | TokenKind::Str(_)
            | TokenKind::Name(_) => i -= 1,
            TokenKind::RParen => {
                let end = i - 1;
                let mut depth = 1usize;
                let mut j = end;
                while j > 0 {
                    j -= 1;
                    match &tokens[j].kind {
                        TokenKind::RParen => depth += 1,
                        TokenKind::LParen => { depth -= 1; if depth == 0 { i = j; break; } }
                        _ => {}
                    }
                }
                if depth != 0 { break; }
            }
            _ => break,
        }
    }
    i
}

fn eval_noun_expr(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "empty noun"));
    }
    if let TokenKind::LParen = &tokens[0].kind {
        if let Some(close) = find_matching_paren(tokens, 0) {
            if close == tokens.len() - 1 {
                return eval_rtl(interp, &tokens[1..close]);
            }
        }
    }
    eval_noun_list(interp, tokens)
}

fn eval_noun_list(interp: &Interpreter, tokens: &[Token]) -> JResult<JVal> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "empty noun"));
    }

    if tokens.len() == 1 {
        return match &tokens[0].kind {
            TokenKind::Number(n)     => Ok(JArray::scalar_int(*n)),
            TokenKind::Float(x)      => Ok(JArray::scalar_float(*x)),
            TokenKind::Complex(r, i) => Ok(JArray::scalar_complex(*r, *i)),
            TokenKind::Str(s)        => Ok(JArray::scalar_int(0)) // TODO: 문자열 타입 (foreign 인자용),
            TokenKind::Name(name) => {
                interp.lookup(name).ok_or_else(|| JError::new(JErrorKind::Value,
                    Some(tokens[0].span.clone()), format!("undefined name: '{}'", name)))
            }
            TokenKind::Verb(v) => {
                let vb = make_primitive(v, &tokens[0].span)?;
                Ok(JArray::from_verb(vb))
            }
            TokenKind::LParen => {
                if let Some(close) = find_matching_paren(tokens, 0) {
                    if close == tokens.len() - 1 {
                        return eval_rtl(interp, &tokens[1..close]);
                    }
                }
                Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[0].span.clone()), "unmatched parenthesis"))
            }
            _ => Err(JError::new(JErrorKind::Syntax,
                Some(tokens[0].span.clone()), "unexpected token")),
        };
    }

    let mut has_float   = false;
    let mut has_complex = false;
    for tok in tokens {
        match &tok.kind {
            TokenKind::Float(_)    => has_float = true,
            TokenKind::Complex(..) => has_complex = true,
            TokenKind::Number(_)   => {}
            TokenKind::Name(name) => {
                return interp.lookup(name).ok_or_else(|| JError::new(JErrorKind::Value,
                    Some(tok.span.clone()), format!("undefined name: '{}'", name)));
            }
            _ => return Err(JError::new(JErrorKind::Syntax,
                Some(tok.span.clone()), "unexpected token in noun list")),
        }
    }

    if has_complex {
        let pairs: Vec<(f64, f64)> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n)     => (*n as f64, 0.0),
            TokenKind::Float(x)      => (*x, 0.0),
            TokenKind::Complex(r, i) => (*r, *i),
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_complex(pairs))
    } else if has_float {
        let data: Vec<f64> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n) => *n as f64,
            TokenKind::Float(x)  => *x,
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_float(data))
    } else {
        let data: Vec<i64> = tokens.iter().map(|tok| match &tok.kind {
            TokenKind::Number(n) => *n,
            _ => unreachable!(),
        }).collect();
        Ok(JArray::vector_int(data))
    }
}

// ─────────────────────────────────────────
// 동사 표현식 파서
// ─────────────────────────────────────────

fn parse_verb_expr(interp: &Interpreter, tokens: &[Token]) -> JResult<VerbBox> {
    if tokens.is_empty() {
        return Err(JError::no_loc(JErrorKind::Syntax, "expected verb expression"));
    }

    // 단일 토큰
    if tokens.len() == 1 {
        return match &tokens[0].kind {
            TokenKind::Verb(v) => make_primitive(v, &tokens[0].span),
            TokenKind::Name(name) => {
                let val = interp.lookup(name).ok_or_else(|| JError::new(JErrorKind::Value,
                    Some(tokens[0].span.clone()), format!("undefined name: '{}'", name)))?;
                val.as_verb().map(Arc::clone).ok_or_else(|| JError::new(JErrorKind::Domain,
                    Some(tokens[0].span.clone()), format!("'{}' is not a verb", name)))
            }
            _ => Err(JError::new(JErrorKind::Syntax,
                Some(tokens[0].span.clone()), "expected verb")),
        };
    }

    // ( verb_expr )
    if let TokenKind::LParen = &tokens[0].kind {
        if let Some(close) = find_matching_paren(tokens, 0) {
            if close == tokens.len() - 1 {
                return parse_verb_expr(interp, &tokens[1..close]);
            }
        }
    }

    // 2토큰 패턴
    if tokens.len() == 2 {
        // verb adverb: +/
        if let (TokenKind::Verb(v), TokenKind::Adverb(adv)) =
            (&tokens[0].kind, &tokens[1].kind)
        {
            let u = make_primitive(v, &tokens[0].span)?;
            return match adv.as_str() {
                "/" => Ok(Arc::new(Slash { u })),
                _   => Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[1].span.clone()), format!("unknown adverb: '{}'", adv))),
            };
        }
        // verb conjunction: + t.
        if let (TokenKind::Verb(v), TokenKind::Conjunction(conj)) =
            (&tokens[0].kind, &tokens[1].kind)
        {
            let u = make_primitive(v, &tokens[0].span)?;
            return match conj.as_str() {
                "t." => Ok(Arc::new(Tco { u })),
                _    => Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[1].span.clone()), format!("unknown conjunction: '{}'", conj))),
            };
        }
    }

    // verb_units 분리
    let verb_units = split_into_verb_units(tokens)?;

    // (verb_expr) conjunction 패턴: (+/ .*) t.
    // split 결과로 이미 처리됨 → parse_verb_expr 재귀에서 처리

    if verb_units.len() == 3 {
        let f = parse_verb_expr(interp, verb_units[0])?;
        let g = parse_verb_expr(interp, verb_units[1])?;
        let h = parse_verb_expr(interp, verb_units[2])?;
        return Ok(Arc::new(Fork { f, g, h }));
    }

    if verb_units.len() == 1 {
        return parse_verb_expr(interp, verb_units[0]);
    }

    let span = tokens[0].span.merge(&tokens[tokens.len()-1].span);
    Err(JError::new(JErrorKind::Syntax, Some(span), "cannot parse verb expression"))
}

/// 토큰 목록을 동사 단위로 분할
///
///   verb                → [verb]
///   verb adverb         → [verb, adverb]      (+/)
///   verb conjunction    → [verb, conj]        (+t.)
///   ( ... )             → [LParen,...,RParen]
///   ( ... ) conjunction → [LParen,...,RParen,conj]  ((+/ .*) t.)
///   name                → [name]
fn split_into_verb_units(tokens: &[Token]) -> JResult<Vec<&[Token]>> {
    let mut units: Vec<&[Token]> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i].kind {
            TokenKind::Verb(_) => {
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
            TokenKind::LParen => {
                if let Some(close) = find_matching_paren(tokens, i) {
                    // ( ... ) conjunction 패턴: (+/ .*) t.
                    if close + 1 < tokens.len() {
                        if let TokenKind::Conjunction(_) = &tokens[close + 1].kind {
                            units.push(&tokens[i..close+2]);
                            i = close + 2;
                            continue;
                        }
                    }
                    units.push(&tokens[i..=close]);
                    i = close + 1;
                } else {
                    return Err(JError::new(JErrorKind::Syntax,
                        Some(tokens[i].span.clone()), "unmatched '('"));
                }
            }
            TokenKind::Adverb(_) => {
                return Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[i].span.clone()), "unexpected adverb"));
            }
            TokenKind::Conjunction(_) => {
                return Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[i].span.clone()), "unexpected conjunction"));
            }
            _ => {
                return Err(JError::new(JErrorKind::Syntax,
                    Some(tokens[i].span.clone()),
                    "unexpected token in verb expression"));
            }
        }
    }

    Ok(units)
}

/// VerbBox 래퍼
#[allow(dead_code)]
struct VerbWrapper(VerbBox);

impl Verb for VerbWrapper {
    fn monad(&self, interp: &Interpreter, w: &JVal) -> JResult<JVal> { self.0.monad(interp, w) }
    fn dyad(&self, interp: &Interpreter, a: &JVal, w: &JVal) -> JResult<JVal> { self.0.dyad(interp, a, w) }
    fn name(&self) -> &str { self.0.name() }
}
