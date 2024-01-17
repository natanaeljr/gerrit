use std::io::Write;
use std::ops::Deref;

use clap::Command;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::{execute, queue};
use trie_rs::{Trie, TrieBuilder};

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
/// - [x] Trim whitespace from user input text
/// - [ ] Make a way to handle '\n' streamed to stdout using print!() as SmartNewLine() instead;
/// - [ ] Match commands with a prefix tree (use trie-rs?) and give completion suggestions.
/// - [ ] On program abort, add hook to restore terminal to normal in order to
///       print panic output message properly new new lines and all.
/// - [ ] SmartMoveLeft: because of wrapped text
///       check for screen column 0 then should MoveUp and MoveToColumn(max).
/// - [ ] SmartPrint: check for new line characters
///
fn main() -> std::io::Result<()> {
    cli::initialize();
    cli::set_prefix("gerrit".to_string().stylize());
    cli::set_symbol(">".to_string().green());

    let mut stdout = cli::stdout();
    cliprintln!(stdout, "Gerrit command-line interface").unwrap();

    let cmd_app = Command::new("gerrit")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .infer_subcommands(true)
        .subcommands([
            Command::new("quit").alias("exit"),
            Command::new("help"),
            Command::new("remote"),
        ]);
    let cmd_tree = get_command_tree(&cmd_app);

    let mut start_input = String::new();
    loop {
        cli::prompt();
        let input = cli::read_inputln(start_input.as_str())?;
        start_input.clear();
        if input.is_empty() {
            continue;
        }

        let cmd_matches_u8: Vec<Vec<u8>> = cmd_tree.predictive_search(input.as_str());
        let cmd_matches: Vec<&str> = cmd_matches_u8
            .iter()
            .map(|u8s| std::str::from_utf8(u8s).unwrap())
            .collect();
        if cmd_matches.is_empty() {
            print_unknown_command(&mut stdout);
            continue;
        }

        // if more than 1 match then suggest command completion
        if cmd_matches.len() > 1 {
            for next_cmd in cmd_matches {
                queue!(stdout, Print(next_cmd), Print("  ")).unwrap();
            }
            execute!(stdout, SmartNewLine(1)).unwrap();
            start_input = input;
            continue;
        }
        // else a full match is found
        let cmd = cmd_matches.last().unwrap().deref();

        // handle command
        match cmd {
            "quit" | "exit" => break,
            "help" | "?" => print_help(&mut stdout, &cmd_app),
            "remote" => cmd_remote(),
            other => print_exception(
                &mut stdout,
                format!("unhandled command! '{}'", other).as_str(),
            ),
        }
    }
    print_done(&mut stdout);
    cli::deinitialize();
    Ok(())
}

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

fn print_help(write: &mut impl Write, cmd_app: &Command) {
    for cmd in cmd_app.get_subcommands() {
        queue!(write, Print(" "), Print(cmd.get_name()), SmartNewLine(1)).unwrap();
        for alias in cmd.get_all_aliases() {
            queue!(write, Print(" "), Print(alias), SmartNewLine(1)).unwrap();
        }
    }
    execute!(write, SmartNewLine(1)).unwrap();
}

fn print_unknown_command(writer: &mut impl Write) {
    execute!(
        writer,
        PrintStyledContent("x".red()),
        Print(" Unknown command"),
        SmartNewLine(1)
    )
    .unwrap();
}

fn print_exception(writer: &mut impl Write, str: &str) {
    execute!(
        writer,
        PrintStyledContent(format!("Exception: {}", str).black().on_red())
    )
    .unwrap();
}

fn print_done(writer: &mut impl Write) {
    execute!(
        writer,
        PrintStyledContent("✓".green()),
        Print(" Done"),
        SmartNewLine(1)
    )
    .unwrap();
}

fn cmd_remote() {
    let mut stdout = cli::stdout();
    execute!(
        stdout,
        Print("remote one"),
        SmartNewLine(1),
        Print("remote two"),
        SmartNewLine(2),
    )
    .unwrap()
}
