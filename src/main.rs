use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{io, thread};

use clap::Command;
use crossterm::cursor::MoveToColumn;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, queue};
use gerlib::changes::{
    AdditionalOpt, ChangeEndpoints, ChangeInfo, Is, QueryOpr, QueryParams, QueryStr, SearchOpr,
};
use gerlib::GerritRestApi;

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
///
fn main() -> std::io::Result<()> {
    cli::initialize();
    cli::set_prefix("gerrit".to_string().stylize());
    cli::set_symbol(">".to_string().green());

    let mut stdout = cli::stdout();
    cliprintln!(stdout, "Gerrit command-line interface").unwrap();

    let cmd_root = Command::new("gerrit")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .subcommands([
            Command::new("quit").alias("exit"),
            Command::new("help"),
            Command::new("remote"),
            Command::new("reset"),
            Command::new("change"),
        ]);

    let url = std::env::var("GERRIT_URL");
    let user = std::env::var("GERRIT_USER");
    let http_pw = std::env::var("GERRIT_PW");
    if url.is_err() || user.is_err() || http_pw.is_err() {
        cliprintln!(stdout, "Please set ENV VARS").unwrap();
        // TODO: cli handle for auto deinitialize (RAII);
        cli::deinitialize();
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

    loop {
        let cmd = cli::prompt(&cmd_root)?;
        match cmd.as_str() {
            "quit" | "exit" => break,
            "help" => print_help(&mut stdout, &cmd_root),
            "remote" => cmd_remote(),
            "change" => cmd_change(&mut gerrit),
            other => print_exception(
                &mut stdout,
                format!("unhandled command! '{}'", other).as_str(),
            ),
        }
    }

    cli::deinitialize();
    Ok(())
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

fn print_exception(writer: &mut impl Write, str: &str) {
    execute!(
        writer,
        PrintStyledContent(format!("Exception: {}", str).black().on_red())
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

fn cmd_change(gerrit: &mut GerritRestApi) {
    let mut stdout = cli::stdout();
    let query_param = QueryParams {
        search_queries: Some(vec![QueryStr::Cooked(vec![
            QueryOpr::Search(SearchOpr::Owner("Natanael.Rabello".to_string())),
            QueryOpr::Search(SearchOpr::Is(Is::Open)),
        ])]),
        additional_opts: Some(vec![
            AdditionalOpt::DetailedAccounts,
            AdditionalOpt::CurrentRevision,
        ]),
        limit: Some(10),
        start: None,
    };
    // TODO: Loading dots square..
    let loading_done = Arc::new(AtomicBool::new(false));
    std::thread::spawn({
        let this_loading_done = loading_done.clone();
        move || {
            let mut stdout = cli::stdout();
            thread::sleep(Duration::from_millis(1000));
            while !this_loading_done.load(Ordering::SeqCst) {
                // TODO: BUG: the . dot may be printed just after this_loading_done is set to true
                // and after the line is cleared.
                execute!(stdout, Print(".")).unwrap();
                thread::sleep(Duration::from_millis(200));
            }
        }
    });
    let changes_list: Vec<Vec<ChangeInfo>> = gerrit.query_changes(&query_param).unwrap();
    loading_done.store(true, Ordering::SeqCst);
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine)).unwrap();

    if changes_list.is_empty() {
        cliprintln!(stdout, "no changes").unwrap();
    }
    for (i, changes) in changes_list.iter().enumerate() {
        for (j, change) in changes.iter().enumerate() {
            queue!(
                stdout,
                PrintStyledContent(format!("{:1}", i + j + 1).blue()),
                Print(" "),
                PrintStyledContent(change.number.to_string().dark_yellow()),
                Print("  "),
                PrintStyledContent(format!("{:3}", change.status).green()),
                Print("  "),
                Print(change.subject.to_string()),
                SmartNewLine(1)
            )
            .unwrap();
        }
    }
    stdout.flush().unwrap();
}
