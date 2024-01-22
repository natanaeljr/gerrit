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

use clap::Command;
use crossterm::cursor::{
    MoveDown, MoveLeft, MoveToColumn, MoveToNextLine, MoveToPreviousLine, MoveUp,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Print, PrintStyledContent, StyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType, ScrollUp};
use crossterm::{cursor, event, execute, queue, style, terminal};
use once_cell::sync::Lazy;
use parking_lot::ReentrantMutex;
use trie_rs::{Trie, TrieBuilder};

use crate::history::HistoryHandle;
use crate::util::TrieUtils;

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

/// CLI guard is a handle for the user.
/// The user should hold this guard as long as it's using the CLI.
/// When `CliGuard` is dropped, the CLI will be deinitialized.
pub struct CliGuard;

/// Initialize the terminal for this CLI shell.
/// This command will configure the terminal to be locked to our shell
/// thus every input is handled from our application only from this point on
pub fn initialize() -> CliGuard {
    let cli_guard = CLI.lock();
    let mut cli = cli_guard.borrow_mut();
    *cli = CliSingleton::default();
    terminal::enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, cursor::Show, style::ResetColor).unwrap();
    CliGuard
}

/// Return the terminal to its normal state.
/// The terminal is unlocked from our application.
/// Input is handled by the terminal from now on and the attributes are reset.
/// The CLI shell is finished and the terminal is free.
fn deinitialize() {
    terminal::disable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    execute!(stdout, cursor::Show, style::ResetColor).unwrap();
    stdout.flush().unwrap();
    // let terminal commands flush for certain
    std::thread::sleep(Duration::from_millis(50));
}

/// Deinitialize the CLI when guard drops.
impl Drop for CliGuard {
    fn drop(&mut self) {
        deinitialize()
    }
}

/// Return the stdout object used for CLI
/// It is centralized here because it can be easier
/// to change if need in the future.
pub fn stdout() -> Stdout {
    std::io::stdout()
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
fn print_prompt() {
    let mut writer = std::io::stdout();
    let curr_col = crossterm::cursor::position().unwrap().0;
    if curr_col > 0 {
        queue!(writer, SmartNewLine(1), Clear(ClearType::CurrentLine)).unwrap();
    }
    let cli_guard = CLI.lock();
    let cli = cli_guard.borrow();
    execute!(
        writer,
        PrintStyledContent(cli.prefix.clone()),
        PrintStyledContent(cli.symbol.clone()),
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
impl crossterm::Command for SmartNewLine {
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

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        if self.0 != 0 {
            let curr_row = crossterm::cursor::position().unwrap().1;
            let term_max_row = crossterm::terminal::size().unwrap().1 - 1;
            if curr_row == term_max_row {
                ScrollUp(self.0).execute_winapi()?;
                MoveUp(self.0).execute_winapi()?;
            }
            sys::move_to_previous_line(self.0)?;
        }
        Ok(())
    }
}

/// Read input from terminal until enter is given.
/// Returns the entered characters until '\n'.
/// This is a fully featured prompt handling with text manipulation
/// just like a shell, with history, arrows handling, backspace, alt, ctrl, etc.
pub fn prompt(cmd_root: &clap::Command) -> std::io::Result<String> {
    let cmd_tree = get_command_tree(&cmd_root);
    let mut history = HistoryHandle::get();
    let mut writer = stdout();
    let mut user_input = String::new();
    let mut last_prompt: Option<String> = None;
    let mut suggestion_printed_below = false;

    print_prompt();
    loop {
        match event::read() {
            // BACKSPACE
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                modifiers,
                state: _,
            })) => {
                if !user_input.is_empty() {
                    let count: u16;
                    if modifiers == KeyModifiers::ALT {
                        if let Some(idx) = user_input.rfind(" ") {
                            // TODO: fix line wrap and overflow
                            count = (user_input.len() - idx) as u16;
                            _ = user_input.split_off(idx);
                        } else {
                            count = user_input.len() as u16;
                            user_input.clear();
                        }
                    } else {
                        user_input.pop();
                        count = 1;
                    }
                    execute!(writer, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                    if suggestion_printed_below {
                        clear_line_below(&mut writer);
                        suggestion_printed_below = false;
                    }
                }
            }
            // TAB
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Tab,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                // TODO: reuse this code with ENTER branch
                if user_input.is_empty() {
                    continue;
                }
                let trimmed_input = user_input.trim().to_string();
                let has_end_whitespace = trimmed_input.len() != user_input.trim_start().len();
                if has_end_whitespace {
                    continue;
                }

                // try to match input string against tree of commands
                let cmd_matches = cmd_tree.collect_matches(&trimmed_input);
                if cmd_matches.is_empty() {
                    continue;
                }

                // if more than one match then suggest command completion
                if cmd_matches.len() > 1 && !has_end_whitespace {
                    let col = cursor::position().unwrap().0;
                    queue!(writer, SmartNewLine(1)).unwrap();
                    print_command_completions(&mut writer, &cmd_matches);
                    execute!(writer, MoveToPreviousLine(1), MoveToColumn(col)).unwrap();
                    suggestion_printed_below = true;
                    continue;
                }

                // else a full match is found
                let cmd = cmd_matches.last().unwrap();
                if trimmed_input.len() < cmd.len() {
                    print_prompt_full_completion(&mut writer, &user_input, &trimmed_input, &cmd);
                    user_input.push_str(cmd.split_at(trimmed_input.len()).1);
                }
                execute!(writer, Print(" ")).unwrap();
                user_input.push(' ');

                if suggestion_printed_below {
                    clear_line_below(&mut writer);
                    suggestion_printed_below = false;
                }
            }
            // ENTER
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if suggestion_printed_below {
                    clear_line_below(&mut writer);
                    suggestion_printed_below = false;
                }
                if user_input.is_empty() {
                    print_prompt();
                    continue;
                }
                let trimmed_input = user_input.trim().to_string();
                let has_end_whitespace = trimmed_input.len() != user_input.trim_start().len();

                // try to match input string against tree of commands
                let cmd_matches = cmd_tree.collect_matches(&trimmed_input);
                if cmd_matches.is_empty() || (cmd_matches.len() > 1 && has_end_whitespace) {
                    queue!(writer, SmartNewLine(1)).unwrap();
                    print_unknown_command(&mut writer);
                    print_prompt();
                    history.add(trimmed_input);
                    user_input.clear();
                    continue;
                }

                // if more than one match then suggest command completion
                if cmd_matches.len() > 1 && !has_end_whitespace {
                    queue!(writer, SmartNewLine(1)).unwrap();
                    print_command_completions(&mut writer, &cmd_matches);
                    print_prompt();
                    execute!(writer, Print(user_input.as_str())).unwrap();
                    continue;
                }

                // else a full match is found
                let cmd = cmd_matches.last().unwrap();
                if trimmed_input.len() < cmd.len() {
                    print_prompt_full_completion(&mut writer, &user_input, &trimmed_input, &cmd);
                }
                // clear any previous line of command suggestions
                execute!(writer, SmartNewLine(1), Clear(ClearType::CurrentLine)).unwrap();

                // command is final, process it now
                history.add(cmd.clone());
                return Ok(cmd.clone());
            }
            // CTRL + C
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(writer, Print("^C"), SmartNewLine(1)).unwrap();
                return Ok(String::from("quit"));
            }
            // CTRL + D
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(writer, Print("^D"), SmartNewLine(1)).unwrap();
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
                execute!(writer, ScrollUp(curr_row), MoveUp(curr_row)).unwrap()
            }
            // ARROW UP
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if let Some(up_next) = history.up_next() {
                    let count = user_input.len() as u16;
                    if last_prompt == None {
                        last_prompt = Some(user_input.clone())
                    }
                    user_input = up_next;
                    if count > 0 {
                        execute!(writer, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    execute!(writer, Print(user_input.as_str())).unwrap();
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
                    let count = user_input.len() as u16;
                    user_input = down_next;
                    if count > 0 {
                        execute!(writer, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                    }
                    execute!(writer, Print(user_input.as_str())).unwrap();
                } else {
                    let count = user_input.len() as u16;
                    if count > 0 {
                        execute!(writer, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    if last_prompt.is_some() {
                        user_input = last_prompt.unwrap();
                        last_prompt = None;
                    }
                    execute!(writer, Print(user_input.as_str())).unwrap();
                }
            }
            // CHARACTERS
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                execute!(writer, Print(c)).unwrap();
                user_input.push(c);
            }
            // ANYTHING
            _ => {}
        }
    }
}

fn print_command_completions(writer: &mut impl Write, cmds: &Vec<String>) {
    for cmd in cmds {
        queue!(writer, Print(cmd), Print("  ")).unwrap();
    }
}

fn print_prompt_full_completion(
    writer: &mut impl Write,
    user_input: &String,
    trimmed_input: &String,
    cmd: &String,
) {
    let whitespace_count = user_input.trim_start().len() - trimmed_input.len();
    if whitespace_count > 0 {
        queue!(writer, MoveLeft(whitespace_count as u16),).unwrap();
    }
    queue!(writer, Print(cmd.split_at(trimmed_input.len()).1)).unwrap();
}

fn clear_line_below(writer: &mut impl Write) {
    execute!(
        writer,
        MoveDown(1),
        Clear(ClearType::CurrentLine),
        MoveUp(1)
    )
    .unwrap();
}

/// TODO: Documentation
fn get_command_tree(cmd_app: &Command) -> Trie<u8> {
    let mut builder = TrieBuilder::new();
    for cmd in cmd_app.get_subcommands() {
        let name = cmd.get_name();
        builder.push(name);
        for alias in cmd.get_all_aliases() {
            builder.push(alias);
        }
    }
    builder.build()
}

/// TODO: Documentation
fn print_unknown_command(writer: &mut impl Write) {
    execute!(
        writer,
        PrintStyledContent("x".red()),
        Print(" Unknown command"),
        SmartNewLine(1)
    )
    .unwrap();
}
