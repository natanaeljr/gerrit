use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::cursor::MoveToColumn;
use crossterm::style::{Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, queue};
use gerlib::changes::{
    AdditionalOpt, ChangeEndpoints, ChangeInfo, Is, QueryOpr, QueryParams, QueryStr, SearchOpr,
};
use gerlib::GerritRestApi;

use crate::cli::SmartNewLine;
use crate::{cli, cliprintln};

/// Handle `change` command.
/// This should enter the `change` mode and print out a list of changes.
pub fn run_cmd(gerrit: &mut GerritRestApi) {
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
