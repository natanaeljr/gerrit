use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::Command;
use crossterm::cursor::MoveToColumn;
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};
use trie_rs::{Trie, TrieBuilder};

use crate::cli;

/// Trait to add $create related functionally to Trie.
pub trait TrieUtils {
    /// Word is the type of collected characters from Trie<T>
    /// Example: Trie<u8> -> Word=String
    type Word;

    /// Get owned collection of matching words for a given prefix from the Trie
    fn collect_matches(&self, prefix: &Self::Word) -> Vec<Self::Word>;
}

impl TrieUtils for Trie<u8> {
    type Word = String;

    fn collect_matches(&self, prefix: &Self::Word) -> Vec<Self::Word> {
        let results_u8: Vec<Vec<u8>> = self.predictive_search(prefix.as_str());
        let results: Vec<String> = results_u8
            .iter()
            .map(|u8s| String::from_utf8(u8s.clone()).unwrap())
            .collect();
        results
    }
}

/// Return a prefix tree of commands based on Command app created with Clap.
/// One can use the command trie to make command predictions.
pub fn get_command_trie(cmd_app: &Command) -> Trie<u8> {
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

/// Return a vector of commands based on Command app created with Clap.
/// One can use the command vector to list all possible commands.
pub fn get_visible_command_vector(cmd_app: &Command) -> Vec<String> {
    let mut vec = Vec::new();
    for cmd in cmd_app.get_subcommands() {
        let name = cmd.get_name().to_string();
        vec.push(name);
        for alias in cmd.get_visible_aliases() {
            vec.push(alias.to_string());
        }
    }
    vec
}

/// Command Action lists actions to taken when returned from command execution
#[derive(PartialEq)]
pub enum CmdAction {
    /// OK = no action
    Ok,
    /// Enter a new CLI mode
    EnterMode(String),
}

/// Search down the command schema for the command string input.
/// The returned command schema corresponds to the last command name in the string.
pub fn find_command<'a>(cmd_schema: &'a Command, inputs: &[String]) -> &'a Command {
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

/// Print loading dots until atomic bool is made true.
/// Useful for commands that take time and want to print some loading symbols to terminal meanwhile.
pub fn loading() -> Arc<AtomicBool> {
    let loading_done = Arc::new(AtomicBool::new(false));
    thread::spawn({
        let this_loading_done = loading_done.clone();
        move || {
            let mut writer = cli::stdout();
            thread::sleep(Duration::from_millis(1000));
            while !this_loading_done.load(Ordering::SeqCst) {
                // TODO: BUG: the . dot may be printed just after this_loading_done is set to true
                // and after the line is cleared.
                execute!(writer, Print(".")).unwrap();
                thread::sleep(Duration::from_millis(200));
            }
        }
    });
    loading_done
}
