//! Gerrit CLI as a shell.
//!
//! This modules manipulates the terminal in order to create a command line interface
//! in such a way that resembles a shell-like program.
//! The following example shows how to use this module's features.
//!
//! # Example:
//! ```
//! fn main() -> io::Result<()> {
//!     cli::initialize();
//!     cli::set_prefix("myprogram".to_string().stylize());
//!     cli::set_symbol(">".to_string().green());
//!     let mut stdout = cli::stdout();
//!     cliprintln!(stdout, "Welcome to MyProgram").unwrap();
//!     loop {
//!         cli::prompt();
//!         let input = cli::read_inputln()?;
//!         if input == "quit" {
//!             break;
//!         }
//!     }
//!     cliprintln!(stdout, "Thanks for stopping by.").unwrap();
//!     cli::deinitialize();
//!     Ok(())
//! }
//! ```
//!
//! ## Output:
//! ```sh
//! user@pc$ myprogram
//! Welcome to MyProgram
//! myprogram>
//! myprogram>quit
//! Thanks for stopping by.
//! user@pc$
//! ```

use std::cell::RefCell;
use std::fmt;
use std::io::{Stdout, Write};
use std::time::Duration;

use crossterm::cursor::{MoveLeft, MoveToNextLine, MoveUp};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Print, PrintStyledContent, StyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType, ScrollUp};
use crossterm::{cursor, event, execute, queue, style, terminal, Command};
use once_cell::sync::Lazy;
use parking_lot::ReentrantMutex;

use crate::history::HistoryHandle;

/// Global variable holding CLI data.
/// It is lazy-initialized on first access.
/// It is thread-safe and can be locked multiple times in the same thread.
/// It is RefCell so that the CLI can be mutable and re-assigned.
static CLI: Lazy<ReentrantMutex<RefCell<CliSingleton>>> =
    Lazy::new(|| ReentrantMutex::new(RefCell::new(CliSingleton::default())));

/// `CliSingleton` holds global CLI data.
/// Nothing fancy, just data that should only have once instance
/// as the CLI is only one per process instance.
struct CliSingleton {
    pub prefix: StyledContent<String>,
    pub symbol: StyledContent<String>,
}

/// Default initialization of `CliSingleton`
impl Default for CliSingleton {
    fn default() -> Self {
        CliSingleton {
            prefix: "cli".to_string().stylize(),
            symbol: ">".to_string().stylize(),
        }
    }
}

/// [`cliprint`] is just a wrapper macro to be able to print a
/// string without having to create a Print object before that.
///
/// [`cliprint`]: crate::cliprint
///
/// Plus arg format works just like `print!()` macro.
///
/// # Example:
/// ```
/// cliprint!(stdout, "{}", Hello World);
/// // same as:
/// execute!(stdout, Print("Hello World");
/// ```
#[macro_export]
macro_rules! cliprint {
    ($writer:expr, $($arg:tt)*) => {{
        execute!($writer, Print(format!($($arg)*)))
    }};
}

/// Just like [`cliprint`] but a smart new line is added at the end.
///
/// [`cliprint`]: crate::cliprint
#[macro_export]
macro_rules! cliprintln {
    ($writer:expr) => {
        execute!($writer, $crate::cli::SmartNewLine(1))
    };
    ($writer:expr, $($arg:tt)*) => {{
        execute!($writer, Print(format!($($arg)*)), $crate::cli::SmartNewLine(1))
    }};
}

/// Initialize the terminal for this CLI shell.
/// This command will configure the terminal to be locked to our shell
/// thus every input is handled from our application only from this point on
pub fn initialize() {
    terminal::enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, cursor::Show, style::ResetColor).unwrap();
}

/// Return the terminal to its normal state.
/// The terminal is unlocked from our application.
/// Input is handled by the terminal from now on and the attributes are reset.
/// The CLI shell is finished and the terminal is free.
pub fn deinitialize() {
    terminal::disable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    execute!(stdout, cursor::Show, style::ResetColor).unwrap();
    stdout.flush().unwrap();
    // let terminal commands flush for certain
    std::thread::sleep(Duration::from_millis(50));
}

/// Return the stdout object used for CLI
/// It is centralized here because it can be easier
/// to change if need in the future.
pub fn stdout() -> Stdout {
    std::io::stdout()
}

/// Update the prompt's prefix string.
/// Prompt will look like this:
/// prefix>
/// where > is the symbol
pub fn set_prefix(p: StyledContent<String>) {
    let cli_guard = CLI.lock();
    let mut cli = cli_guard.borrow_mut();
    cli.prefix = p;
}

/// Update the prompt's symbol string.
/// Prompt will look like this:
/// prefix>
/// where > is the symbol
pub fn set_symbol(s: StyledContent<String>) {
    let cli_guard = CLI.lock();
    let mut cli = cli_guard.borrow_mut();
    cli.symbol = s;
}

/// Print prompt for user input
/// This will display the configured `prefix>` in a blank line as a shell prompt.
pub fn prompt() {
    let mut stdout = std::io::stdout();
    let curr_col = crossterm::cursor::position().unwrap().0;
    if curr_col > 0 {
        queue!(stdout, SmartNewLine(1), Clear(ClearType::CurrentLine)).unwrap();
    }
    let cli_guard = CLI.lock();
    let cli = cli_guard.borrow();
    execute!(
        stdout,
        PrintStyledContent(cli.prefix.clone()),
        PrintStyledContent(cli.symbol.clone())
    )
    .unwrap();
}

/// Check if we are at the last row in the terminal,
/// then we may need to scroll up because we are in RAW mode,
/// and the terminal won't do that automatically in this mode.
/// This Command quietly does that before `MoveToNextLine`.
/// Then return the new line object, so this function can be used inside
/// execute! or queue! in place of the actual `MoveToNextLine` object.
pub struct SmartNewLine(pub u16);

/// Implementation of the SmartNewLine that handles next-line + scroll.
impl Command for SmartNewLine {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        let curr_row = crossterm::cursor::position().unwrap().1;
        let term_max_row = crossterm::terminal::size().unwrap().1 - 1;
        if curr_row == term_max_row {
            ScrollUp(self.0).write_ansi(f)?;
            MoveUp(self.0).write_ansi(f)?;
        }
        MoveToNextLine(self.0).write_ansi(f)?;
        Ok(())
    }

    // #[cfg(windows)]
    // fn execute_winapi(&self) -> std::io::Result<()> {
    //     if self.0 != 0 {
    //         sys::move_to_previous_line(self.0)?;
    //     }
    //     Ok(())
    // }
}

/// Read input from terminal until enter is given.
/// Returns the entered characters until '\n'.
/// This is a fully featured prompt handling with text manipulation
/// just like a shell, with history, arrows handling, backspace, alt, ctrl, etc.
pub fn read_inputln() -> std::io::Result<String> {
    let mut stdout = stdout();
    let mut history = HistoryHandle::get();
    let mut prompt = String::new();
    let mut last_prompt: Option<String> = None;
    loop {
        match event::read() {
            // BACKSPACE
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                modifiers,
                state: _,
            })) => {
                if !prompt.is_empty() {
                    let count: u16;
                    if modifiers == KeyModifiers::ALT {
                        if let Some(idx) = prompt.rfind(" ") {
                            // TODO: fix line wrap and overflow
                            count = (prompt.len() - idx) as u16;
                            _ = prompt.split_off(idx);
                        } else {
                            count = prompt.len() as u16;
                            prompt.clear();
                        }
                    } else {
                        prompt.pop();
                        count = 1;
                    }
                    execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                }
            }
            // ENTER
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if !prompt.is_empty() {
                    prompt = prompt.trim().to_string();
                    history.add(prompt.clone());
                }
                return Ok(prompt);
            }
            // CTRL + C
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^C")).unwrap();
                return Ok(String::from("quit"));
            }
            // CTRL + D
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^D")).unwrap();
                return Ok(String::from("quit"));
            }
            // CTRL + L
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('l'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                let curr_row = crossterm::cursor::position().unwrap().1;
                execute!(stdout, ScrollUp(curr_row), MoveUp(curr_row)).unwrap()
            }
            // ARROW UP
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if let Some(up_next) = history.up_next() {
                    let count = prompt.len() as u16;
                    if last_prompt == None {
                        last_prompt = Some(prompt.clone())
                    }
                    prompt = up_next;
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                }
            }
            // ARROW DOWN
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if let Some(down_next) = history.down_next() {
                    let count = prompt.len() as u16;
                    prompt = down_next;
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                } else {
                    let count = prompt.len() as u16;
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    if last_prompt.is_some() {
                        prompt = last_prompt.unwrap();
                        last_prompt = None;
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                }
            }
            // CHARACTERS
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                execute!(stdout, Print(c)).unwrap();
                prompt.push(c);
            }
            // ANYTHING
            _ => {}
        }
    }
}
