use std::io::Write;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::Command;
use crossterm::cursor::{MoveToColumn, MoveToPreviousLine};
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, queue};
use gerlib::changes::{
    AdditionalOpt, ChangeEndpoints, ChangeInfo, Is, QueryOpr, QueryParams, QueryStr, SearchOpr,
};
use gerlib::GerritRestApi;
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
            Command::new("change"),
        ]);
    let cmd_tree = get_command_tree(&cmd_app);

    let mut gerrit = GerritRestApi::new("url".parse().unwrap(), "username", "password")
        .unwrap()
        .ssl_verify(false)
        .unwrap();

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
        execute!(stdout, MoveToPreviousLine(1)).unwrap();
        cli::prompt();
        execute!(stdout, Print(cmd), SmartNewLine(1)).unwrap();

        // handle command
        match cmd {
            "quit" | "exit" => break,
            "help" | "?" => print_help(&mut stdout, &cmd_app),
            "remote" => cmd_remote(),
            "change" => cmd_change(&mut gerrit),
            other => print_exception(
                &mut stdout,
                format!("unhandled command! '{}'", other).as_str(),
            ),
        }
    }
    // print_done(&mut stdout);
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
        limit: Some(20),
        start: None,
    };
    // TODO: Loading dots square..
    let loading_done = Arc::new(AtomicBool::new(false));
    let loading_thread = std::thread::spawn({
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
    for changes in &changes_list {
        for change in changes {
            queue!(
                stdout,
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
    execute!(stdout, SmartNewLine(1), MoveToPreviousLine(1)).unwrap();
}
