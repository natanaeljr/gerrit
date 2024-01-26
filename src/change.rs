use std::cell::RefCell;
use std::io::Write;
use std::str::FromStr;
use std::sync::atomic::Ordering;

use clap::{Arg, Command};
use crossterm::cursor::MoveToColumn;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, queue};
use gerlib::changes::{
    AdditionalOpt, ChangeEndpoints, ChangeInfo, Is, QueryOpr, QueryParams, QueryStr, SearchOpr,
};
use gerlib::GerritRestApi;
use once_cell::sync::Lazy;
use parking_lot::ReentrantMutex;

use crate::cli::SmartNewLine;
use crate::util::CmdAction;
use crate::{cli, cliprintln, print_help, util};

static CHANGE_CONTEXT: Lazy<ReentrantMutex<RefCell<ChangeContext>>> =
    Lazy::new(|| ReentrantMutex::new(RefCell::new(ChangeContext::default())));

#[derive(Default)]
struct ChangeContext {
    list: Vec<ChangeInfo>,
}

/// Get the `change` command model/schema as a Clap command structure
pub fn command() -> Command {
    Command::new("change")
        .disable_version_flag(true)
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .about("Change commands")
        .subcommands([
            Command::new("show")
                .arg(Arg::new("ID").required(true))
                .about("Display change info"),
            Command::new("list").about("List changes"),
            Command::new("load").about("Load temporary command"),
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
    let (cmd, cmd_args) = args.split_first().unwrap();
    match cmd.as_str() {
        "show" => show_change(cmd_args, gerrit),
        "list" => list_changes(cmd_args, gerrit),
        "help" | "?" => {
            print_help(&mut writer, &command());
            Ok(CmdAction::Ok)
        }
        "exit" => Ok(CmdAction::Ok),
        _ => Err(()),
    }
}

/// Print out a list of changes.
pub fn list_changes(_args: &[String], gerrit: &mut GerritRestApi) -> Result<CmdAction, ()> {
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
    let loading_done = util::loading();
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

    let ctx_guard = CHANGE_CONTEXT.lock();
    let mut ctx = ctx_guard.borrow_mut();
    ctx.list = changes_list.into_iter().flatten().collect();

    Ok(CmdAction::Ok)
}

/// Display change info
pub fn show_change(args: &[String], gerrit: &mut GerritRestApi) -> Result<CmdAction, ()> {
    let mut writer = cli::stdout();

    if args.len() != 1 {
        cliprintln!(writer, "Required ID argument").unwrap();
        return Ok(CmdAction::Ok);
    }

    let mut id = args.last().unwrap().clone();
    let mut id_is_index = false;
    if id.starts_with("$") {
        id = id.split_off(1);
        id_is_index = true;
    }
    let id_u32 = match u32::from_str(id.as_str()) {
        Ok(id) => id,
        Err(_) => {
            cliprintln!(writer, "Argument is not a number").unwrap();
            return Ok(CmdAction::Ok);
        }
    };

    if id_is_index {
        let ctx_guard = CHANGE_CONTEXT.lock();
        let ctx = ctx_guard.borrow();
        if id_u32 == 0 {
            cliprintln!(writer, "ID out of bounds").unwrap();
            return Ok(CmdAction::Ok);
        }
        if let Some(change) = ctx.list.get(id_u32 as usize - 1) {
            id = change.number.to_string();
        } else {
            cliprintln!(writer, "ID out of bounds").unwrap();
            return Ok(CmdAction::Ok);
        }
    }

    let additional_opts = vec![
        AdditionalOpt::CurrentRevision,
        AdditionalOpt::CurrentCommit,
        AdditionalOpt::CurrentFiles,
        AdditionalOpt::DetailedAccounts,
        AdditionalOpt::DetailedLabels,
    ];
    let loading_done = util::loading();
    let change = gerrit
        .get_change(id.as_str(), Some(additional_opts))
        .unwrap();
    loading_done.store(true, Ordering::SeqCst);
    execute!(writer, MoveToColumn(0), Clear(ClearType::CurrentLine)).unwrap();

    queue!(
        writer,
        PrintStyledContent(change.number.to_string().dark_yellow()),
        Print("  "),
        PrintStyledContent(format!("{:3}", change.status).green()),
        Print("  "),
        Print(change.subject.to_string()),
        SmartNewLine(1)
    )
    .unwrap();

    queue!(writer, Print(&change.change_id), SmartNewLine(1)).unwrap();

    let curr_rev_id = change.current_revision.as_ref().unwrap();
    let curr_rev_info = change.revisions.as_ref().unwrap().get(curr_rev_id).unwrap();
    let curr_commit_info = curr_rev_info.commit.as_ref().unwrap();
    let curr_commit_msg = curr_commit_info.message.as_ref().unwrap();

    queue!(writer, SmartNewLine(1)).unwrap();
    let lines = curr_commit_msg.lines();
    for line in lines {
        queue!(writer, Print("    "), Print(line), SmartNewLine(1)).unwrap();
    }

    execute!(writer, SmartNewLine(1)).unwrap();
    Ok(CmdAction::Ok)
}
