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

    let cmd_schema_root = command();
    let mut fixed_args = Vec::new();
    loop {
        let curr_cmd_schema = find_command(&cmd_schema_root, fixed_args.as_slice());
        let new_args = cli::prompt(curr_cmd_schema)?;
        if new_args.is_empty() {
            continue;
        }
        // first level commands
        let cmd = new_args.first().unwrap();
        match cmd.as_str() {
            "quit" => break,
            "exit" => {
                if fixed_args.is_empty() {
                    break;
                } else {
                    fixed_args.clear();
                    cli::set_prefix("gerrit".to_string().stylize());
                    continue;
                }
            }
            _ => {}
        }
        // fixed args defined by mode are joined with new args and
        // handled down the command tree path as an all-in-one input line from user
        let mut all_args = fixed_args.clone();
        all_args.extend_from_slice(new_args.as_slice());
        // second level commands
        let subcmd_ret = run_subcommand(all_args.as_slice(), &mut gerrit);
        if let Ok(action) = subcmd_ret {
            match action {
                CmdAction::Ok => {}
                CmdAction::EnterMode(str) => {
                    fixed_args = all_args;
                    cli::set_prefix(str.stylize());
                }
            }
            continue;
        }
        // registered command was not handled
        let exception = format!("unhandled command! '{}'", cmd);
        print_exception(&mut writer, exception.as_str());
    }
    Ok(())
}

fn find_command<'a>(cmd_schema: &'a Command, inputs: &[String]) -> &'a Command {
    let mut curr_cmd = cmd_schema;
    for input in inputs {
        let new_cmd = curr_cmd
            .get_subcommands()
            .find(|c| c.get_name() == input)
            .unwrap();
        curr_cmd = new_cmd;
    }
    curr_cmd
}

/// Get the `gerrit` command model/schema as a Clap command structure
fn command() -> Command {
    Command::new("gerrit")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .subcommands([
            change::command(),
            Command::new("remote").about("Remote commands"),
            Command::new("reset").about("Reset everything temporarily"),
            Command::new("help").alias("?").about("Print command help"),
            Command::new("exit").about("Exit from current mode"),
            Command::new("quit").about("Quit the program"),
        ])
}

#[derive(PartialEq)]
enum CmdAction {
    Ok,
    EnterMode(String),
}

/// Match prompt against subcommands.
/// Run matched subcommand and return result.
fn run_subcommand(args: &[String], gerrit: &mut GerritRestApi) -> Result<CmdAction, ()> {
    if args.is_empty() {
        return Ok(CmdAction::Ok);
    }
    let (cmd, cmd_args) = args.split_first().unwrap();
    match cmd.as_str() {
        "remote" => remote_run_command(),
        "change" => change::run_command(cmd_args, gerrit),
        "help" | "?" => {
            print_help(&mut cli::stdout(), &command());
            Ok(CmdAction::Ok)
        }
        _ => Err(()),
    }
}

/// Display help
/// This should basically print out the command list and that's it.
fn print_help(write: &mut impl Write, cmd_app: &Command) {
    for cmd in cmd_app.get_subcommands() {
        let line = format!(
            " {:6}       {}",
            cmd.get_name(),
            cmd.get_about().unwrap_or_default()
        );
        queue!(write, Print(line), SmartNewLine(1)).unwrap();
        for alias in cmd.get_visible_aliases() {
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
fn remote_run_command() -> Result<CmdAction, ()> {
    let mut stdout = cli::stdout();
    let url = std::env::var("GERRIT_URL");
    if let Ok(url) = url {
        execute!(stdout, Print("remote url: "), Print(url), SmartNewLine(1),).unwrap()
    } else {
        cliprintln!(stdout, "no remotes configured").unwrap()
    }
    Ok(CmdAction::Ok)
}
