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
use std::ops::ControlFlow;
use std::time::Duration;

use crossterm::cursor::{
    MoveDown, MoveLeft, MoveToColumn, MoveToNextLine, MoveToPreviousLine, MoveUp,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Print, PrintStyledContent, StyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType, ScrollUp};
use crossterm::{cursor, event, execute, queue, style, terminal};
use once_cell::sync::Lazy;
use parking_lot::ReentrantMutex;

use crate::history::HistoryHandle;
use crate::util;
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
pub fn prompt(cmd_schema: &clap::Command) -> std::io::Result<Vec<String>> {
    let mut history = HistoryHandle::get();
    let mut writer = stdout();
    let mut user_input = String::new();
    let mut last_prompt: Option<String> = None;
    let mut suggestion_printed_below = false;

    print_prompt();
    'prompt_loop: loop {
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
                        let index = util::str_rfind_last_word_separator(user_input.as_str());
                        count = (user_input.len() - index) as u16;
                        // execute!(
                        //     writer,
                        //     MoveDown(1),
                        //     Print(format!("index {} count {}", index, count)),
                        //     MoveUp(1)
                        // )
                        // .unwrap();
                        _ = user_input.split_off(index);
                    } else {
                        user_input.pop();
                        count = 1;
                    }
                    if count > 0 {
                        execute!(writer, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                    }
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
                if suggestion_printed_below {
                    clear_line_below(&mut writer);
                    suggestion_printed_below = false;
                }

                if user_input.is_empty() {
                    let cmds = util::get_visible_command_vector(&cmd_schema);
                    let col = cursor::position().unwrap().0;
                    queue!(writer, SmartNewLine(1)).unwrap();
                    print_command_completions(&mut writer, &cmds);
                    execute!(writer, MoveToPreviousLine(1), MoveToColumn(col)).unwrap();
                    suggestion_printed_below = true;
                    continue;
                }

                let mut curr_cmd_schema = cmd_schema;
                let mut user_input_offset = 0;
                let mut new_user_input = user_input.clone();
                let user_input2 = user_input.clone();
                let mut cmd_arg_given = false;
                for (word_idx, word_input) in user_input2
                    .split_whitespace()
                    .map(|str| (str.as_ptr() as usize - user_input2.as_ptr() as usize, str))
                {
                    let cmd_arg = curr_cmd_schema.get_arguments().next();

                    let word_input = word_input.to_string();
                    let has_end_whitespace = user_input2
                        .chars()
                        .nth(word_idx + word_input.len())
                        .map_or_else(|| false, |c| c.is_whitespace());

                    // try to match input string against tree of commands or arguments
                    let cmd_trie = if cmd_arg.is_some() {
                        util::get_arg_values_trie(&cmd_arg.unwrap())
                    } else {
                        util::get_command_trie(&curr_cmd_schema)
                    };

                    let cmd_matches = cmd_trie.collect_matches(&word_input);
                    if cmd_matches.is_empty() || (cmd_matches.len() > 1 && has_end_whitespace) {
                        let col = cursor::position().unwrap().0;
                        queue!(writer, SmartNewLine(1)).unwrap();
                        print_invalid_input(&mut writer, &word_input);
                        execute!(writer, MoveToPreviousLine(2), MoveToColumn(col)).unwrap();
                        suggestion_printed_below = true;
                        continue 'prompt_loop;
                    }

                    // if more than one match then suggest command completion
                    if cmd_matches.len() > 1 && !has_end_whitespace {
                        let col = cursor::position().unwrap().0;
                        queue!(writer, SmartNewLine(1)).unwrap();
                        print_command_completions(&mut writer, &cmd_matches);
                        execute!(writer, MoveToPreviousLine(1), MoveToColumn(col)).unwrap();
                        suggestion_printed_below = true;
                        continue 'prompt_loop;
                    }

                    // else a full match is found
                    let cmd = cmd_matches.last().unwrap();
                    if word_input.len() < cmd.len() {
                        let word_end_idx = word_idx + word_input.len() + user_input_offset;
                        let cmd_remainder = cmd.split_at(word_input.len()).1;
                        user_input_offset += cmd_remainder.len();
                        new_user_input.insert_str(word_end_idx, cmd_remainder);
                        // print_prompt_full_completion(&mut writer, &user_input, &word_input, &cmd);
                    }

                    // command is final, process it now

                    if cmd_arg.is_some() {
                        cmd_arg_given = true;
                    } else {
                        curr_cmd_schema = curr_cmd_schema
                            .get_subcommands()
                            .find(|c| {
                                c.get_name() == cmd
                                    || c.get_all_aliases().find(|a| a == cmd) != None
                            })
                            .unwrap();
                    }
                }

                if user_input.ends_with(" ")
                    && (curr_cmd_schema.get_subcommands().next().is_some()
                        || curr_cmd_schema.get_arguments().next().is_some())
                {
                    let cmds = if curr_cmd_schema.get_subcommands().next().is_some() {
                        util::get_visible_command_vector(&curr_cmd_schema)
                    } else {
                        util::get_arg_values_vector(curr_cmd_schema.get_arguments().next().unwrap())
                    };
                    let col = cursor::position().unwrap().0;
                    queue!(writer, SmartNewLine(1)).unwrap();
                    print_command_completions(&mut writer, &cmds);
                    execute!(writer, MoveToPreviousLine(1), MoveToColumn(col)).unwrap();
                    suggestion_printed_below = true;
                    continue 'prompt_loop;
                }

                if user_input != new_user_input {
                    execute!(writer, MoveToColumn(0)).unwrap();
                    print_prompt();
                    execute!(writer, Print(new_user_input.as_str())).unwrap();
                    execute!(writer, Print(" ")).unwrap();
                    user_input = new_user_input.clone();
                    user_input.push(' ');
                    continue 'prompt_loop;
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
                let mut args = Vec::new();
                let mut curr_cmd_schema = cmd_schema;
                let mut user_input_offset = 0;
                let mut new_user_input = user_input.clone();
                let user_input2 = user_input.clone();
                let mut cmd_arg_given = false;
                for (word_idx, word_input) in user_input2
                    .split_whitespace()
                    .map(|str| (str.as_ptr() as usize - user_input2.as_ptr() as usize, str))
                {
                    let cmd_arg = curr_cmd_schema.get_arguments().next();
                    if cmd_arg.is_some() && cmd_arg.unwrap().get_possible_values().is_empty() {
                        args.push(word_input.to_string());
                        cmd_arg_given = true;
                        continue;
                    }

                    let word_input = word_input.to_string();
                    let has_end_whitespace = user_input2
                        .chars()
                        .nth(word_idx + word_input.len())
                        .map_or_else(|| false, |c| c.is_whitespace());

                    // try to match input string against tree of commands or arguments
                    let cmd_trie = if cmd_arg.is_some() {
                        util::get_arg_values_trie(&cmd_arg.unwrap())
                    } else {
                        util::get_command_trie(&curr_cmd_schema)
                    };

                    let cmd_matches = cmd_trie.collect_matches(&word_input);
                    if cmd_matches.is_empty() || (cmd_matches.len() > 1 && has_end_whitespace) {
                        queue!(writer, SmartNewLine(1)).unwrap();
                        print_invalid_input(&mut writer, &word_input);
                        print_prompt();
                        history.add(new_user_input);
                        user_input.clear();
                        continue 'prompt_loop;
                    }

                    // if more than one match then suggest command completion
                    if cmd_matches.len() > 1 && !has_end_whitespace {
                        queue!(writer, SmartNewLine(1)).unwrap();
                        print_command_completions(&mut writer, &cmd_matches);
                        print_prompt();
                        execute!(writer, Print(user_input.as_str())).unwrap();
                        continue 'prompt_loop;
                    }

                    // else a full match is found
                    let cmd = cmd_matches.last().unwrap();
                    if word_input.len() < cmd.len() {
                        let word_end_idx = word_idx + word_input.len() + user_input_offset;
                        let cmd_remainder = cmd.split_at(word_input.len()).1;
                        user_input_offset += cmd_remainder.len();
                        new_user_input.insert_str(word_end_idx, cmd_remainder);
                        // print_prompt_full_completion(&mut writer, &user_input, &word_input, &cmd);
                    }

                    // command is final, process it now
                    args.push(cmd.clone());

                    if cmd_arg.is_some() {
                        cmd_arg_given = true;
                    } else {
                        curr_cmd_schema = curr_cmd_schema
                            .get_subcommands()
                            .find(|c| {
                                c.get_name() == cmd
                                    || c.get_all_aliases().find(|a| a == cmd) != None
                            })
                            .unwrap();
                    }
                }
                execute!(writer, MoveToColumn(0)).unwrap();
                print_prompt();
                execute!(writer, Print(new_user_input.as_str())).unwrap();
                // clear any previous line of command suggestions
                execute!(writer, SmartNewLine(1), Clear(ClearType::CurrentLine)).unwrap();
                history.add(new_user_input.trim().to_string());

                let cli_arg = curr_cmd_schema.get_arguments().next();
                if cli_arg.is_some() && cli_arg.unwrap().is_required_set() && !cmd_arg_given {
                    cliprintln!(writer, "Missing argument");
                    print_prompt();
                    user_input.clear();
                    continue;
                }

                return Ok(args);
            }

            // CTRL + C
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(writer, Print("^C"), SmartNewLine(1)).unwrap();
                print_prompt();
                user_input.clear();
            }

            // CTRL + D
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                if user_input.is_empty() {
                    execute!(writer, Print("^D"), SmartNewLine(1)).unwrap();
                    return Ok(vec![String::from("exit")]);
                }
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

/// Print out list of commands as for completion suggestions.
/// TODO: support line wrapping after newline tracking is implemented.
fn print_command_completions(writer: &mut impl Write, cmds: &Vec<String>) {
    for cmd in cmds {
        queue!(writer, Print(cmd), Print("  ")).unwrap();
    }
}

/// Complete user prompt with remainder of command string
/// This will print only remaining characters.
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

/// Clear line below and return to previous line
fn clear_line_below(writer: &mut impl Write) {
    execute!(
        writer,
        MoveDown(1),
        Clear(ClearType::CurrentLine),
        MoveUp(1)
    )
    .unwrap();
}

/// Print out message "Unknown command" with new line
fn print_invalid_input(writer: &mut impl Write, input: &str) {
    execute!(
        writer,
        PrintStyledContent("x".red()),
        Print(" Invalid input: "),
        Print(input),
        SmartNewLine(1)
    )
    .unwrap();
}

struct Prompt {
    writer: Stdout,
    history: HistoryHandle,
    user_input: String,
    last_prompt: Option<String>,
    suggestion_printed_below: bool,
}

impl Prompt {
    pub fn new() -> Self {
        Self {
            writer: stdout(),
            history: HistoryHandle::get(),
            user_input: String::new(),
            last_prompt: None,
            suggestion_printed_below: false,
        }
    }

    pub fn prompt(&mut self) -> std::io::Result<Vec<String>> {
        loop {
            let control_flow = match event::read()? {
                Event::Key(event) => self.key_event(event),
                _ => ControlFlow::Continue(()),
            };
            if let ControlFlow::Break(input) = control_flow {
                return Ok(input);
            }
        }
    }

    fn key_event(&mut self, event: KeyEvent) -> ControlFlow<Vec<String>> {
        match event {
            KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                ..
            } => self.backspace(event),
            _ => ControlFlow::Continue(()),
        }
    }

    fn backspace(&mut self, event: KeyEvent) -> ControlFlow<Vec<String>> {
        if self.user_input.is_empty() {
            return ControlFlow::Continue(());
        }
        let num_of_chars_to_clear: u16;
        if event.modifiers == KeyModifiers::ALT {
            if let Some(idx) = self.user_input.rfind(" ") {
                // TODO: fix line wrap and overflow
                num_of_chars_to_clear = (self.user_input.len() - idx) as u16;
                _ = self.user_input.split_off(idx);
            } else {
                num_of_chars_to_clear = self.user_input.len() as u16;
                self.user_input.clear();
            }
        } else {
            self.user_input.pop();
            num_of_chars_to_clear = 1;
        }
        execute!(
            self.writer,
            MoveLeft(num_of_chars_to_clear),
            Clear(ClearType::UntilNewLine)
        )
        .unwrap();
        if self.suggestion_printed_below {
            clear_line_below(&mut self.writer);
            self.suggestion_printed_below = false;
        }
        ControlFlow::Continue(())
    }
}

pub fn prompt2(cmd_schema: &clap::Command) -> std::io::Result<Vec<String>> {
    Prompt::new().prompt()
}
