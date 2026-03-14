//! ```cargo
//! [dependencies]
//! rustyline = "15.0"
//! ```

use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};

fn main() -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                println!("Line: {}", line);
            }, 
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                println!("Error: {:?}", err);
                break
            }
        }
    }
    Ok(())
}
