use std::sync::RwLock;

use once_cell::sync::Lazy;

/// The command-line history is composed by a global history.
/// Right now, history is reset every time the program is invoked,
/// because `HISTORY` is a static global variable.
/// We have a `HistoryHandle` to manipulate the scroll through the history lines
/// every time a block of code wants to have history access.

/// This is the prompts history storage.
/// It's global because easier to handle right now.
/// Because of that, all code manipulating `HISTORY` needs to acquire RW lock
/// in order to safely access the inner data. Hence `HISTORY` is thread safe.
/// Thus use `HistoryHandle` as wrapper for safe code and to provide utility functions.
static HISTORY: Lazy<RwLock<Vec<String>>> = Lazy::new(|| RwLock::default());

/// `HistoryHandle` will scroll through the history lines and update `HISTORY`.
/// Thus an index is kept to know where up in the history we have scrolled through.
/// User of the HistoryHandle can `add` new lines to the history and scroll through the history
/// using `up_next` and `down_next` to get previous and latest lines.
pub struct HistoryHandle {
    curr_index: usize,
}

impl HistoryHandle {
    /// Get a new `HistoryHandle` to manipulate `HISTORY`.
    pub fn get() -> Self {
        let history = HISTORY.read().unwrap();
        Self {
            curr_index: history.len(),
        }
    }

    /// Add new line to the `HISTORY`.
    /// This is a smart add because history will not duplicate
    /// the last prompt line if it's added multiple times.
    pub fn add(&mut self, new_line: String) {
        let mut history = HISTORY.write().unwrap();
        if let Some(last_line) = history.last() {
            if &new_line == last_line {
                return;
            }
        }
        history.push(new_line);
    }

    /// Get previous line from `HISTORY` just above current index.
    /// This will update current index in the scroll.
    pub fn up_next(&mut self) -> Option<String> {
        let history = HISTORY.read().unwrap();
        if self.curr_index == 0 || history.is_empty() {
            return None;
        }
        self.curr_index -= 1;
        history.get(self.curr_index).cloned()
    }

    /// Get last line from `HISTORY` just below current index.
    /// This will update current index in the scroll.
    pub fn down_next(&mut self) -> Option<String> {
        let history = HISTORY.read().unwrap();
        if self.curr_index >= history.len() {
            return None;
        }
        self.curr_index += 1;
        history.get(self.curr_index).cloned()
    }
}
