// ═══════════════════════════════════════════════════════════════════════════
// Word Formation State Machine (adapted from jsource/jsrc/w.c)
// ═══════════════════════════════════════════════════════════════════════════
//
// This state machine processes input characters to identify word tokens.
// Each state represents the parser's current context, and transitions depend
// on the character class of the input.

// ───────────────────────────────────────────────────────────────────────────
// STATE ABBREVIATIONS (S prefix = State)
// ───────────────────────────────────────────────────────────────────────────
// SS    = Space (default state)
// SS9   = Space (but previous field was numeric - affects followon handling)
// SX    = Other/Non-alphanumeric character
// SA    = Alphanumeric character
// SN    = Character 'N' (beginning of potential comment marker)
// SNB   = 'NB' sequence detected
// SQQ   = Even number of quotes (end of quoted string)
// S9    = Numeric digit
// S99   = Numeric (and previous field was numeric - followon numeric)
// SQ    = Inside a quoted string (odd number of quotes)
// SNZ   = 'NB.' comment marker detected
// SZ    = Inside a trailing comment (everything after NB.)
// SU    = Previous char was uninflectable/special (e.g., newline/LF)
// SDD   = Single '{' brace seen
// SDDZ  = Single '}' brace seen
// SDDD  = Double braces '{{' or '}}' seen

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserState {
    Space = 0,           // SS
    SpaceAfterNumeric = 1,// SS9
    Other = 2,           // SX
    Alphanumeric = 3,    // SA
    N = 4,               // SN
    NB = 5,              // SNB
    EvenQuotes = 6,      // SQQ
    Numeric = 7,         // S9
    NumericFollowon = 8, // S99
    Quote = 9,           // SQ
    NB_Dot = 10,         // SNZ
    TrailComment = 11,   // SZ
    Uninflectable = 12,  // SU
    BraceOpen = 13,      // SDD
    BraceClose = 14,     // SDDZ
    BraceDouble = 15,    // SDDD
}

// ───────────────────────────────────────────────────────────────────────────
// ACTION CODES (E prefix = Emit/Action)
// ───────────────────────────────────────────────────────────────────────────
// E0  = No action (continue without emitting)
// EI  = End of previous word - emit it (when transitioning to space)
// EN  = Start of next word - save position
// EZ  = End and start together - emit current word AND begin next word

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitAction {
    None = 0,    // E0 - no action
    Emit = 1,    // EI/EN - either end or start
    Both = 2,    // EZ - end+start together
}

// ───────────────────────────────────────────────────────────────────────────
// INPUT CHARACTER CLASS ABBREVIATIONS (C prefix = Character class)
// ───────────────────────────────────────────────────────────────────────────
// CX    = Unknown/other character
// CDD   = Left brace '{' (open brace)
// CDDZ  = Right brace '}' (close brace)
// CU    = Uninflectable character (LF/CR, line terminator)
// CS    = Space character
// CA    = Alphabetic character
// CN    = Character 'N'
// CB    = Character 'B'
// C9    = Numeric digit (0-9)
// CD    = Decimal point '.'
// CC    = Comment marker (':' in J)
// CQ    = Quote character (single quote)

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Other = 0,      // CX
    BraceOpen = 1,  // CDD
    BraceClose = 2, // CDDZ
    Uninflect = 3,  // CU
    Space = 4,      // CS
    Alpha = 5,      // CA
    CharN = 6,      // CN
    CharB = 7,      // CB
    Digit = 8,      // C9
    Decimal = 9,    // CD
    Comment = 10,   // CC
    Quote = 11,     // CQ
}

// ───────────────────────────────────────────────────────────────────────────
// STATE MACHINE TABLE
// ───────────────────────────────────────────────────────────────────────────
// Dimensions: [16 states] x [12 character classes]
// Value format: (NextState, EmitAction)
// Special: UNDD flag indicates inflection handling for doubled braces

pub const STATE_MACHINE: &[&[(ParserState, EmitAction)]] = &[
    // SS (Space state)
    &[(Other, Start), (BraceOpen, Start), (BraceClose, Start), (Uninflectable, Start),
      (Space, None), (Alphanumeric, Start), (N, Start), (Alphanumeric, Start),
      (Numeric, Start), (Other, None), (Other, None), (Quote, Start)],
    
    // SS9 (Space after numeric)
    &[(Other, Start), (BraceOpen, Start), (BraceClose, Start), (Uninflectable, Start),
      (SpaceAfterNumeric, None), (Alphanumeric, Start), (N, Start), (Alphanumeric, Start),
      (NumericFollowon, Start), (Other, None), (Other, None), (Quote, Start)],
    
    // SX (Other character state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, None), (Other, None), (Quote, Both)],
    
    // SA (Alphanumeric state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, None), (Alphanumeric, None), (Alphanumeric, None),
      (Alphanumeric, None), (Other, None), (Other, None), (Quote, Both)],
    
    // SN (Character 'N' state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, None), (Alphanumeric, None), (NB, None),
      (Alphanumeric, None), (Other, None), (Other, None), (Quote, Both)],
    
    // SNB (NB sequence detected)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, None), (Alphanumeric, None), (Alphanumeric, None),
      (Alphanumeric, None), (NB_Dot, None), (Other, None), (Quote, Both)],
    
    // SQQ (Even quotes - string closed)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, Both), (Other, Both), (Quote, None)],
    
    // S9 (Numeric state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (SpaceAfterNumeric, Emit), (Numeric, None), (Numeric, None), (Numeric, None),
      (Numeric, None), (Numeric, None), (Other, None), (Quote, Both)],
    
    // S99 (Numeric followon state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (SpaceAfterNumeric, Emit), (NumericFollowon, None), (NumericFollowon, None), (NumericFollowon, None),
      (NumericFollowon, None), (NumericFollowon, None), (Other, None), (Quote, Both)],
    
    // SQ (Inside quotes)
    &[(Quote, None), (Quote, None), (Quote, None), (Quote, None),
      (Quote, None), (Quote, None), (Quote, None), (Quote, None),
      (Quote, None), (Quote, None), (Quote, None), (EvenQuotes, None)],
    
    // SNZ (NB. detected)
    &[(TrailComment, None), (TrailComment, None), (TrailComment, None), (Uninflectable, Both),
      (TrailComment, None), (TrailComment, None), (TrailComment, None), (TrailComment, None),
      (TrailComment, None), (Other, None), (Other, None), (TrailComment, None)],
    
    // SZ (Trailing comment state)
    &[(TrailComment, None), (TrailComment, None), (TrailComment, None), (Uninflectable, Both),
      (TrailComment, None), (TrailComment, None), (TrailComment, None), (TrailComment, None),
      (TrailComment, None), (TrailComment, None), (TrailComment, None), (TrailComment, None)],
    
    // SU (Uninflectable state)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, Both), (Other, Both), (Quote, Both)],
    
    // SDD (Single { brace)
    &[(Other, Both), (BraceDouble, None), (BraceOpen, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, None), (Other, None), (Quote, Both)],
    
    // SDDZ (Single } brace)
    &[(Other, Both), (BraceOpen, Both), (BraceDouble, None), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, None), (Other, None), (Quote, Both)],
    
    // SDDD (Double {{ or }} braces)
    &[(Other, Both), (BraceOpen, Both), (BraceClose, Both), (Uninflectable, Both),
      (Space, Emit), (Alphanumeric, Both), (N, Both), (Alphanumeric, Both),
      (Numeric, Both), (Other, None), (Other, None), (Quote, Both)],
];

// ═══════════════════════════════════════════════════════════════════════════
// Token Validation Macros & Rules (from jsource/jsrc/w.c jtenqueue function)
// ═══════════════════════════════════════════════════════════════════════════
//
// After the state machine creates word boundaries, the enqueue function validates
// each token to determine its semantic type and handle special cases like
// inflections, primitives, and assignment operators.

// ───────────────────────────────────────────────────────────────────────────
// TOKEN PROPERTY CHECKING MACROS
// ───────────────────────────────────────────────────────────────────────────

/// Check if word at index i has NAME attribute
/// Examines the type flags of the current word
/// Example: TNAME(0) checks if first word is a name
pub fn is_name(word: &JWord) -> bool {
    word.attr_type.contains(AttrType::NAME)
}

/// Check if word at index i has ASGN (assignment) attribute
/// Used to identify assignment operators (=, =:, etc)
pub fn is_assignment(word: &JWord) -> bool {
    word.attr_type.contains(AttrType::ASGN)
}

/// Check if word at index i matches a specific verb/primitive
/// Compares word block address with shared block for given character
/// Example: TVERB(i, CCOMMA) checks if word is comma ',' verb
pub fn is_verb(word: &JWord, verb_code: char) -> bool {
    word.primitive_code == verb_code
}

/// Check if word at index i is right brace '}'
pub fn is_rbrace(word: &JWord) -> bool {
    is_verb(word, '>')  // CRBRACE verb code
}

/// Check if two words at indices i and j are equal names
/// Used for pattern matching in special forms
pub fn names_equal(word_i: &JWord, word_j: &JWord) -> bool {
    is_name(word_i) && 
    is_name(word_j) && 
    word_i.name == word_j.name
}

// ───────────────────────────────────────────────────────────────────────────
// FIRST CHARACTER TYPE ANALYSIS
// ───────────────────────────────────────────────────────────────────────────
// After the state machine, tokens are classified by their first character

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstCharClass {
    // Printable ASCII range (32-127)
    Ascii32to127 = 0,

    // Special starts
    Digit = 1,          // C9: numeric digit (0-9)
    Alpha = 2,          // CA: alphabetic character - will be NAME
    Quote = 3,          // CQ: single quote ' - string constant
    Other = 4,          // CX: other non-alphanumeric
}

/// Classify first character of a token for validation
pub fn classify_first_char(c: u8) -> FirstCharClass {
    match c {
        b'0'..=b'9' => FirstCharClass::Digit,
        b'a'..=b'z' | b'A'..=b'Z' | b'_' => FirstCharClass::Alpha,
        b'\'' => FirstCharClass::Quote,
        32..=127 => FirstCharClass::Ascii32to127,
        _ => FirstCharClass::Other,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// INFLECTION HANDLING
// ───────────────────────────────────────────────────────────────────────────
// J allows inflections on words: trailing . or : indicating verb/adverb/noun variants

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflectionMarker {
    Dot = 0,     // CESC1 = '.' - marks adverb/conjunction
    Colon = 1,   // CESC2 = ':' - marks definition/locative
    None = 2,    // No inflection
}

/// Check if last character indicates an inflection
/// Returns the inflection type if found, None otherwise
pub fn get_inflection(word: &str) -> InflectionMarker {
    if word.len() >= 2 {
        match word.as_bytes()[word.len() - 1] {
            b'.' => InflectionMarker::Dot,
            b':' => InflectionMarker::Colon,
            _ => InflectionMarker::None,
        }
    } else {
        InflectionMarker::None
    }
}

// ───────────────────────────────────────────────────────────────────────────
// SPELL-IN TABLES (port of ws.c's spellintab2 / spellintab3)
// ───────────────────────────────────────────────────────────────────────────
// The original C implementation in ws.c used two large static tables to
// convert two‑ and three‑character primitive spellings into their canonical
// single‑character codes.  In Rust we keep the same structure; the tables
// are initially filled with zeros and should be populated with the values
// from ws.c during initialization or (preferably) at compile time.  For
// now the arrays are declared here along with helper lookup functions.

// 256×256 table for two‑character sequences.  An entry of `0` means "no
// mapping"; otherwise the byte value is the canonical primitive code.
//
// We declare the arrays `mut` so that they may be filled at startup by a
// one‑time initialization routine that translates the values from the
// original `ws.c` tables.  In production you may replace this with a
// compile‑time generated `const` array once the data is known.
static mut SPELLINTAB2: [[u8; 256]; 256] = [[0; 256]; 256];

// 256×256×256 table for three‑character sequences.  The value semantics are
// the same as for the two‑character table.
static mut SPELLINTAB3: [[[u8; 256]; 256]; 256] = [[[0; 256]; 256]; 256];

/// Look up a two‑character spelling in the table.  Returns `0` if no entry
/// exists.
#[inline]
pub fn spellin2(a: u8, b: u8) -> u8 {
    // SAFETY: table is initialized before use (see `populate_spellin_tables`).
    unsafe { SPELLINTAB2[a as usize][b as usize] }
}

/// Look up a three‑character spelling in the table.  Returns `0` if no entry
/// exists.
#[inline]
pub fn spellin3(a: u8, b: u8, c: u8) -> u8 {
    // SAFETY: table is initialized before use (see `populate_spellin_tables`).
    unsafe { SPELLINTAB3[a as usize][b as usize][c as usize] }
}

/// Convert a raw token (already stripped of any inflection marker) into the
/// canonical primitive string that should be looked up in `lookup_primitive`.
///
/// The algorithm mirrors the behaviour of the C `spellin` function:
/// 1. If the first three bytes form a recognised three‑character primitive,
///    return a one‑byte string containing its canonical code.
/// 2. Otherwise, try a two‑character lookup.
/// 3. Fall back to the original string if there is no special mapping.
///
/// The tables themselves must be populated from the J source; currently they
/// are zeroed and this function therefore behaves as the identity.
/// Populate the two‑ and three‑character tables with the data copied from
/// the J source (`ws.c`).  This should be called once during program
/// initialization **before** any tokenization takes place.  The body below is
/// only an illustration; the real implementation must move the literal data
/// from the C arrays into the Rust tables.
///
/// # Safety
///
/// Calling this function more than once or after tokens have already been
/// validated is harmless but unnecessary.  The function performs `unsafe`
/// writes to `static mut` arrays.
pub fn populate_spellin_tables() {
    unsafe {
        // Example entries; replace with the full table exported from ws.c.
        // In ws.c the two‑character table was declared as `US spellintab2[256][256]`
        // and the three‑character as `US spellintab3[256][256][256]`.
        SPELLINTAB2[b'=' as usize][b':' as usize] = b'='; // "=:", canonical = '='
        SPELLINTAB2[b'<' as usize][b'.' as usize] = b'<'; // "<.", canonical = '<'
        // ... more entries copied verbatim ...

        SPELLINTAB3[b'<' as usize][b':' as usize][b':' as usize] = b'<';
        //  ... etc. ...
    }
}

pub fn spellin(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() >= 3 {
        let code = spellin3(bytes[0], bytes[1], bytes[2]);
        if code != 0 {
            return (code as char).to_string();
        }
    }
    if bytes.len() >= 2 {
        let code = spellin2(bytes[0], bytes[1]);
        if code != 0 {
            return (code as char).to_string();
        }
    }
    raw.to_string()
}

// ───────────────────────────────────────────────────────────────────────────
// PRIMITIVE LOOKUP & VALIDATION
// ───────────────────────────────────────────────────────────────────────────
// Map word text (possibly with inflections) to primitive code

#[derive(Debug, Clone)]
pub struct PrimitiveEntry {
    /// Character representation of the primitive (e.g., '+', '-', '=')
    pub code: u8,
    
    /// Primitive type (verb, noun, adverb, conjunction)
    pub prim_type: PrimitiveType,
    
    /// Whether this primitive has usecount (is permanent/built-in)
    pub is_permanent: bool,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Verb = 0,       // +, -, *, etc.
    Noun = 1,       // Constants like 0, 1, etc.
    Adverb = 2,     // / (reduce), \ (scan), etc.
    Conjunction = 3,// : (definition), = (assignment), etc.
}

/// Look up word text after inflection processing in primitive table
/// Returns primitive entry if found, None otherwise
/// 
/// Example: "+" -> PrimitiveEntry { code: '+', prim_type: Verb, is_permanent: true }
///          "+:" (inflected) -> same as "+"
pub fn lookup_primitive(word_text: &str) -> Option<PrimitiveEntry> {
    // This table exists in J as shared permanent blocks
    // Each primitive is registered at initialization time
    // For Rust, we would build this from a static hash map or match statement
    match word_text {
        "+" => Some(PrimitiveEntry {
            code: b'+',
            prim_type: PrimitiveType::Verb,
            is_permanent: true,
        }),
        "-" => Some(PrimitiveEntry {
            code: b'-',
            prim_type: PrimitiveType::Verb,
            is_permanent: true,
        }),
        "=" => Some(PrimitiveEntry {
            code: b'=',
            prim_type: PrimitiveType::Conjunction,
            is_permanent: true,
        }),
        // ... more primitives ...
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// TOKEN VALIDATION FUNCTION
// ───────────────────────────────────────────────────────────────────────────
// Main validation logic that processes each token from the state machine

#[derive(Debug, Clone)]
pub enum TokenValidationResult {
    /// Primitive word (built-in verb/noun/adverb/conjunction)
    Primitive(PrimitiveEntry),
    
    /// User-defined name (must match identifier rules)
    Name(String),
    
    /// Numeric constant
    Numeric(String),
    
    /// String constant
    String(String),
    
    /// Invalid token with error description
    Invalid(String),
}

/// Validate a token after state machine processing
/// 
/// Process:
/// 1. Get first character and classify it
/// 2. Check for inflections on last character
/// 3. Convert inflected form to canonical primitive code (spellin)
/// 4. Look up in primitive table
/// 5. If not primitive, process by first-char type:
///    - Alpha (32-127): Must be valid NAME (identifier rules)
///    - Digit: Parse numeric constant
///    - Quote: String constant already processed
///    - Other: Error - invalid start character
pub fn validate_token(
    word_text: &str,
    original_pos: usize,
    env: TokenEnvironment,
) -> TokenValidationResult {
    if word_text.is_empty() {
        return TokenValidationResult::Invalid("empty token".to_string());
    }

    let first_char = word_text.as_bytes()[0];
    let has_inflection = get_inflection(word_text) != InflectionMarker::None;
    let inflection_chars_to_remove = if has_inflection { 2 } else { 0 };
    
    // Try to convert to primitive form (with inflection handling)
    let raw = &word_text[..word_text.len() - inflection_chars_to_remove];
    let canonical_form = spellin(raw);
    
    // Check if it's a primitive
    if first_char >= 32 && first_char <= 127 {
        if let Some(prim) = lookup_primitive(&canonical_form) {
            return TokenValidationResult::Primitive(prim);
        }
    }
    
    // Not a primitive, classify by first character
    match classify_first_char(first_char) {
        FirstCharClass::Alpha => {
            // Must be valid NAME - check identifier rules
            if is_valid_identifier(word_text) {
                TokenValidationResult::Name(word_text.to_string())
            } else {
                TokenValidationResult::Invalid(format!("invalid name: {}", word_text))
            }
        }
        FirstCharClass::Digit => {
            // Parse as numeric constant
            TokenValidationResult::Numeric(word_text.to_string())
        }
        FirstCharClass::Quote => {
            // String constant - quotes already removed by state machine
            TokenValidationResult::String(word_text.to_string())
        }
        FirstCharClass::Ascii32to127 => {
            // Must be a primitive (but we already checked above)
            TokenValidationResult::Invalid(format!(
                "invalid start character '{}' at position {}",
                first_char as char,
                original_pos
            ))
        }
        FirstCharClass::Other => {
            TokenValidationResult::Invalid(format!(
                "non-ASCII character '{}' at position {}",
                first_char, original_pos
            ))
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Helper types and functions

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEnvironment {
    /// env=0: Tacit translator context
    TacitTranslator = 0,
    
    /// env=1: Keyboard/IMMEX with no local variables
    KeyboardNoLocals = 1,
    
    /// env=2: Explicit definition execution
    ExplicitDefn = 2,
}

/// Check if a string is a valid J identifier
/// Valid: starts with letter, followed by letters, digits, underscores
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    let first = s.as_bytes()[0];
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    
    s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

// ───────────────────────────────────────────────────────────────────────────
// Special Pattern Matching (In-place operations optimization)
// ───────────────────────────────────────────────────────────────────────────
// After validation, check for special sentence patterns for optimization
// Pattern: abc=: pqr}x,...y,:z (assignment with operation)

pub struct SpecialFormPattern {
    /// Variable being assigned
    pub var_name: String,
    /// Operation name
    pub operation: String,
    /// Arguments
    pub args: Vec<String>,
}

/// Check if validated tokens match pattern: name ASGN name VERB args
pub fn try_match_inplace_pattern(tokens: &[TokenValidationResult]) -> Option<SpecialFormPattern> {
    // Implementation would analyze the token sequence
    // Pattern: odd length >= 7, position 0 is NAME, position 1 is ASGN, etc.
    // This is used for optimizing array update operations
    None
}


// ---------------------------------------------------------------------------
// unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellin_identity() {
        assert_eq!(spellin("hello"), "hello");
    }

    #[test]
    fn spellin_two_char_mapping() {
        // set up a trivial mapping and exercise it
        populate_spellin_tables();
        unsafe {
            SPELLINTAB2[b'A' as usize][b'B' as usize] = b'X';
        }
        assert_eq!(spellin("AB"), "X");
        // fallback when no mapping exists
        assert_eq!(spellin("CD"), "CD");
    }

    #[test]
    fn spellin_three_char_mapping() {
        populate_spellin_tables();
        unsafe {
            SPELLINTAB3[b'A' as usize][b'B' as usize][b'C' as usize] = b'Y';
        }
        assert_eq!(spellin("ABC"), "Y");
    }
}

