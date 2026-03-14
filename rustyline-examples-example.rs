i//! ```cargo
//! [dependencies]
//! rustyline = "15.0"
//! 
//! bitflags = "2.11"
//! cfg-if = "1.0.4"
# For file completion
home = { version = "0.5.12", optional = true }
# For History
rusqlite = { version = "0.38.0", optional = true, default-features = false, features = [
    "bundled",
    "cache",
    "backup",
    "fallible_uint",
] }
libc = "0.2.180"
log = "0.4.29"
unicode-width = "0.2.2"
unicode-segmentation = "1.12"
memchr = "2.8"
# For custom bindings
radix_trie = { version = "0.3", optional = true }
regex = { version = "1.12.3", optional = true }
# For derive
rustyline-derive = { version = "0.11.1", optional = true, path = "rustyline-derive" }

[target.'cfg(unix)'.dependencies]
nix = { version = "0.31.1", default-features = false, features = [
    "fs",
    "ioctl",
    "poll",
    "signal",
    "term",
] }
utf8parse = "0.2"
skim = { version = "3.3.0", optional = true, default-features = false }
signal-hook = { version = "0.4.3", optional = true, default-features = false }
termios = { version = "0.3.3", optional = true }
buffer-redux = { version = "1.1", optional = true, default-features = false }

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61.2", features = [
    "Win32_Foundation",
    "Win32_System_Console",
    "Win32_Security",
    "Win32_System_Threading",
    "Win32_UI_Input_KeyboardAndMouse",
] }
clipboard-win = "5.4"

[dev-dependencies]
doc-comment = "0.3"
env_logger = { version = "0.11", default-features = false }
tempfile = "3.25.0"
rand = "0.10"
assert_matches = "1.5"bitflags = "2.11"
cfg-if = "1.0.4"
# For file completion
home = { version = "0.5.12", optional = true }
# For History
rusqlite = { version = "0.38.0", optional = true, default-features = false, features = [
    "bundled",
    "cache",
    "backup",
    "fallible_uint",
] }
libc = "0.2.180"
log = "0.4.29"
unicode-width = "0.2.2"
unicode-segmentation = "1.12"
memchr = "2.8"
# For custom bindings
radix_trie = { version = "0.3", optional = true }
regex = { version = "1.12.3", optional = true }
# For derive
rustyline-derive = { version = "0.11.1", optional = true, path = "rustyline-derive" }

[target.'cfg(unix)'.dependencies]
nix = { version = "0.31.1", default-features = false, features = [
    "fs",
    "ioctl",
    "poll",
    "signal",
    "term",
] }
utf8parse = "0.2"
skim = { version = "3.3.0", optional = true, default-features = false }
signal-hook = { version = "0.4.3", optional = true, default-features = false }
termios = { version = "0.3.3", optional = true }
buffer-redux = { version = "1.1", optional = true, default-features = false }

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61.2", features = [
    "Win32_Foundation",
    "Win32_System_Console",
    "Win32_Security",
    "Win32_System_Threading",
    "Win32_UI_Input_KeyboardAndMouse",
] }
clipboard-win = "5.4"

[dev-dependencies]
doc-comment = "0.3"
env_logger = { version = "0.11", default-features = false }
tempfile = "3.25.0"
rand = "0.10"
assert_matches = "1.5"

use std::borrow::Cow::{self, Owned};

use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter, MatchingBracketHighlighter};
use rustyline::hint::HistoryHinter;
use rustyline::validate::MatchingBracketValidator;
use rustyline::{Cmd, CompletionType, Config, EditMode, Editor, KeyEvent};
use rustyline::{Completer, Helper, Hinter, Validator};

#[derive(Helper, Completer, Hinter, Validator)]
struct MyHelper {
    #[rustyline(Completer)]
    completer: FilenameCompleter,
    highlighter: MatchingBracketHighlighter,
    #[rustyline(Validator)]
    validator: MatchingBracketValidator,
    #[rustyline(Hinter)]
    hinter: HistoryHinter,
}

impl Highlighter for MyHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned("\x1b[1m".to_owned() + hint + "\x1b[m")
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.highlighter.highlight_char(line, pos, kind)
    }
}

// To debug rustyline:
// RUST_LOG=rustyline=debug cargo run --example example 2> debug.log
fn main() -> rustyline::Result<()> {
    env_logger::init();
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();
    let h = MyHelper {
        completer: FilenameCompleter::new(),
        highlighter: MatchingBracketHighlighter::new(),
        hinter: HistoryHinter::new(),
        validator: MatchingBracketValidator::new(),
    };
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));
    rl.bind_sequence(KeyEvent::alt('n'), Cmd::HistorySearchForward);
    rl.bind_sequence(KeyEvent::alt('p'), Cmd::HistorySearchBackward);
    if rl.load_history("history.txt").is_err() {
        println!("No previous history.");
    }
    let mut count = 1;
    loop {
        let p = format!("{count}> ");
        let colored_prompt = format!("\x1b[1;32m{p}\x1b[0m");
        let readline = rl.readline(&(p, colored_prompt));
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                println!("Line: {line}");
            }
            Err(ReadlineError::Interrupted) => {
                println!("Interrupted");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Encountered Eof");
                break;
            }
            Err(err) => {
                println!("Error: {err:?}");
                break;
            }
        }
        count += 1;
    }
    rl.append_history("history.txt")
}
//! ```
