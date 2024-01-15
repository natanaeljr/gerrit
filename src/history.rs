/// The command-line history is composed by a global history.
/// Right now, history is reset every time the program is invoked,
/// because `HISTORY` is a static global variable.
/// We have a `HistoryHandle` to manipulate the scroll through the history lines
/// every time a block of code wants to have history access.

/// This is the prompts history storage.
/// It's global because easier to handle right now.
/// Because of that, all code manipulating `HISTORY` will be unsafe.
/// Thus use `HistoryHandle` as wrapper for safe code and to provide utility functions.
static mut HISTORY: Vec<String> = Vec::new();

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
        unsafe {
            Self {
                curr_index: HISTORY.len(),
            }
        }
    }

    /// Add new line to the `HISTORY`.
    /// This is a smart add because history will not duplicate
    /// the last prompt line if it's added multiple times.
    pub fn add(&mut self, new_line: String) {
        unsafe {
            if let Some(last_line) = HISTORY.last() {
                if &new_line == last_line {
                    return;
                }
            }
            HISTORY.push(new_line);
        }
    }

    /// Get previous line from `HISTORY` just above current index.
    /// This will update current index in the scroll.
    pub fn up_next(&mut self) -> Option<&String> {
        unsafe {
            if self.curr_index == 0 || HISTORY.is_empty() {
                return None;
            }
            self.curr_index -= 1;
            HISTORY.get(self.curr_index)
        }
    }

    /// Get last line from `HISTORY` just below current index.
    /// This will update current index in the scroll.
    pub fn down_next(&mut self) -> Option<&String> {
        unsafe {
            if self.curr_index >= HISTORY.len() {
                return None;
            }
            self.curr_index += 1;
            if self.curr_index == HISTORY.len() {
                return None;
            }
            HISTORY.get(self.curr_index)
        }
    }
}
