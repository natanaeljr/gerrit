use std::io::Write;

use crossterm::execute;
use crossterm::style::{Print, PrintStyledContent, Stylize};

use crate::cli::SmartNewLine;

mod cli;
mod history;

/// The ideia right now is to create a binary to start testing crossterm again
/// and re-create the ger CLI from scratch.
/// This new version will be similar to network CLIs like confd and ocnos and bluetoothctl.
/// Example:
/// gerrit> help
/// gerrit> remote
/// gerrit> quit
///
/// Next step:
/// - [ ] Handle commands with Clap::App
/// - [x] Handle scroll when cursor is at last row of the terminal window
/// - [ ] Command History (clear HISTORY, navegate HISTORY, print HISTORY, auto save/load HISTORY)
/// - [ ] Clear command should clear all lines up to the start of the command `gerrit`
///       that means, clear until where the command `gerrit` was invoked.
///       example:
///       user@pc$ # other stuff          user@pc$ # other stuff
///       user@pc$ gerrit                 user@pc$ gerrit
///       gerrit> fdsfds      ---->>>     gerrit>
///       gerrit> abc
///       gerrit> clear
///
///       This command is kind of complicated because it has to:
///       Keep track of the new lines that were printed.
///       Also include the MoveUp, MoveDown... Scroll into the calcule of
///       lines added from the begging of the program until now.
///       ScrollDown until program invokation line will be required.
///       Clear all lines below it will be required.
/// - [ ] Script as input to run automatically commands from a file
/// - [x] HISTORY up/down with on-going command restore on last down-arrow
/// - [ ] Handle left/right arrows and prompt in-middle insert characters,
///       prompt will have to shift the characters.
/// - [ ] Trim whitespace from user input text
///
fn main() -> std::io::Result<()> {
    cli::initialize();
    cli::set_prefix("gerrit".to_string().stylize());
    cli::set_symbol(">".to_string().green());

    let mut stdout = cli::stdout();
    cliprintln!(stdout, "Gerrit command-line interface");

    let mut quit = false;
    while !quit {
        cli::prompt();
        let input = cli::read_inputln()?;
        match input.as_str() {
            "quit" | "exit" => quit = true,
            "help" | "?" => print_help(&mut stdout),
            "remote" => cmd_remote(),
            str if !str.is_empty() => {
                print_unknown_command(&mut stdout);
            }
            _ => {}
        }
    }
    print_done(&mut stdout);
    cli::deinitialize();
    Ok(())
}

pub fn print_help(write: &mut impl Write) {
    execute!(
        write,
        SmartNewLine(1),
        Print(" help"),
        SmartNewLine(1),
        Print(" remote"),
        SmartNewLine(1),
        Print(" quit"),
        SmartNewLine(2),
    )
    .unwrap()
}

pub fn print_unknown_command(writer: &mut impl Write) {
    execute!(
        writer,
        SmartNewLine(1),
        PrintStyledContent("x".red()),
        Print(" Unknown command"),
        SmartNewLine(1)
    )
    .unwrap();
}

pub fn print_done(writer: &mut impl Write) {
    execute!(
        writer,
        SmartNewLine(1),
        PrintStyledContent("✓".green()),
        Print(" Done"),
        SmartNewLine(1)
    )
    .unwrap();
}

pub fn cmd_remote() {
    let mut stdout = cli::stdout();
    execute!(
        stdout,
        SmartNewLine(1),
        Print("remote one"),
        SmartNewLine(1),
        Print("remote two"),
        SmartNewLine(2),
    )
    .unwrap()
}
