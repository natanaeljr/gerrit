use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
use crate::util::CmdAction;
use crate::{cli, cliprintln, print_help};

/// Get the `change` command model/schema as a Clap command structure
pub fn command() -> Command {
    Command::new("change")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .about("Change commands")
        .subcommands([
            Command::new("show").about("Display change info"),
            Command::new("list").about("List changes"),
            Command::new("line").about("Line temporary command"),
            Command::new("help").alias("?").about("Print command help"),
            Command::new("exit").about("Exit from current mode"),
            Command::new("quit").about("Quit the program"),
        ])
}

/// Handle `change` command.
pub fn run_command(args: &[String], gerrit: &mut GerritRestApi) -> Result<CmdAction, ()> {
    let mut writer = cli::stdout();
    if args.is_empty() {
        return Ok(CmdAction::EnterMode("gerrit change".to_string()));
    }
    let (cmd, _cmd_args) = args.split_first().unwrap();
    match cmd.as_str() {
        "show" => {
            cliprintln!(writer, "Show changes").unwrap();
            Ok(CmdAction::Ok)
        }
        "list" => list_changes(gerrit),
        "help" | "?" => {
            print_help(&mut writer, &command());
            Ok(CmdAction::Ok)
        }
        "exit" => Ok(CmdAction::Ok),
        _ => Err(()),
    }
}

/// Print out a list of changes.
pub fn list_changes(gerrit: &mut GerritRestApi) -> Result<CmdAction, ()> {
    let mut writer = cli::stdout();
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
    execute!(writer, MoveToColumn(0), Clear(ClearType::CurrentLine)).unwrap();

    if changes_list.is_empty() {
        cliprintln!(writer, "no changes").unwrap();
    }
    for (i, changes) in changes_list.iter().enumerate() {
        for (j, change) in changes.iter().enumerate() {
            queue!(
                writer,
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
    writer.flush().unwrap();
    Ok(CmdAction::Ok)
}
