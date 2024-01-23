use std::fmt::Display;
use std::io;
use std::io::{ErrorKind, Write};

use clap::Command;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::{execute, queue};
use gerlib::GerritRestApi;

use crate::cli::SmartNewLine;

mod change;
mod cli;
mod history;
mod util;

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
///       
///       Idea: Track new lines moves in SmartNewLine and SmartPrevLine.
///       Keep a new line count in CLI global struct and create cli::clear function
///       that abstracts the functionally.
///
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
/// - [ ] Pass command list as param to cli::read_inputln()
///       to make cli suggestion and completion-on-enter a library function of Cli.
///       We can then save the full command name in history, and a full match is found.
/// - [ ] TAB command completion
/// - [ ] Cli mode set. Example 'gerrit>change<CR>' -> 'change>'
/// - [ ] Directly run commands from program invocation args (main args) and quit.
/// - [ ] Display auto logged-in user and remote info in a Banner from program start
///       Similar to linux login info banner.
///       Create login auto start config for enabling that.
/// - [ ] Maybe this prefix+symbol could be a func param only of prompt();
/// - [ ] Cache `change` cmd output and allow list of changes to be referenced by
///       index in following commands. Example:
///         gerrit>change
///         1 139924  NEW  Changing header color
///         2 139721  NEW  New footer design
///         3 139453  NEW  Support new SDK version
///         gerrit>show #1
/// - [ ] Read & Run commands from stdin, then quit.
///       Example: echo -e 'change' | gerrit
///
fn main() -> std::io::Result<()> {
    let _cli_guard = cli::initialize();
    cli::set_prefix("gerrit".to_string().stylize());
    cli::set_symbol(">".to_string().green());

    let mut writer = cli::stdout();
    cliprintln!(writer, "Gerrit command-line interface").unwrap();

    let url = std::env::var("GERRIT_URL");
    let user = std::env::var("GERRIT_USER");
    let http_pw = std::env::var("GERRIT_PW");
    if url.is_err() || user.is_err() || http_pw.is_err() {
        cliprintln!(writer, "Please set ENV VARS").unwrap();
        return Err(io::Error::from(ErrorKind::PermissionDenied));
    }

    let mut gerrit = GerritRestApi::new(
        url.unwrap().parse().unwrap(),
        user.unwrap().as_str(),
        http_pw.unwrap().as_str(),
    )
    .unwrap()
    .ssl_verify(false)
    .unwrap();

    let cmd_schema = command();
    loop {
        let args = cli::prompt(&cmd_schema)?;
        if args.is_empty() {
            continue;
        }
        let cmd = args.first().unwrap();
        // first level commands
        match cmd.as_str() {
            "quit" | "exit" => break,
            _ => {}
        }
        // second level commands
        if run_subcommand(args.as_slice(), &mut gerrit).is_ok() {
            continue;
        }
        // registered command was not handled
        let exception = format!("unhandled command! '{}'", cmd);
        print_exception(&mut writer, exception.as_str());
    }
    Ok(())
}

/// Get the `gerrit` command model/schema as a Clap command structure
fn command() -> Command {
    Command::new("gerrit")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .subcommands([
            Command::new("quit").alias("exit"),
            Command::new("help"),
            Command::new("remote"),
            Command::new("reset"),
            change::command(),
        ])
}

/// Match prompt against subcommands.
/// Run matched subcommand and return result.
fn run_subcommand(args: &[String], gerrit: &mut GerritRestApi) -> Result<(), ()> {
    if args.is_empty() {
        return Ok(());
    }
    let (cmd, args2) = args.split_first().unwrap();
    match cmd.as_str() {
        "remote" => remote_run_command(),
        "change" => change::run_command(args2, gerrit),
        "help" => {
            print_help(&mut cli::stdout(), &command());
            Ok(())
        }
        _ => Err(()),
    }
}

/// Display help
/// This should basically print out the command list and that's it.
fn print_help(write: &mut impl Write, cmd_app: &Command) {
    for cmd in cmd_app.get_subcommands() {
        queue!(write, Print(" "), Print(cmd.get_name()), SmartNewLine(1)).unwrap();
        for alias in cmd.get_all_aliases() {
            queue!(write, Print(" "), Print(alias), SmartNewLine(1)).unwrap();
        }
    }
    execute!(write, SmartNewLine(1)).unwrap();
}

/// Print out an exception message in highlight.
fn print_exception<D: Display>(writer: &mut impl Write, str: D) {
    execute!(
        writer,
        PrintStyledContent(format!("Exception: {}", str).black().on_red())
    )
    .unwrap();
}

/// Handle `remote` command.
/// NOTE: Temporary function place.
fn remote_run_command() -> Result<(), ()> {
    let mut stdout = cli::stdout();
    let url = std::env::var("GERRIT_URL");
    if let Ok(url) = url {
        execute!(stdout, Print("remote url: "), Print(url), SmartNewLine(1),).unwrap()
    } else {
        cliprintln!(stdout, "no remotes configured").unwrap()
    }
    Ok(())
}
