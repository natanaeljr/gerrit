static mut HISTORY: Vec<String> = Vec::new();

pub struct HistoryHandle {
    curr_index: usize,
}

impl HistoryHandle {
    pub fn get() -> Self {
        unsafe {
            Self {
                curr_index: HISTORY.len(),
            }
        }
    }

    pub fn add(&mut self, new_line: String) {
        unsafe {
            if let Some(last_line) = HISTORY.last() {
                if &new_line == last_line {
                    return;
                }
            }
            HISTORY.push(new_line);
            self.curr_index = HISTORY.len();
        }
    }

    pub fn up_next(&mut self) -> Option<&String> {
        unsafe {
            if self.curr_index == 0 || HISTORY.is_empty() {
                return None;
            }
            self.curr_index -= 1;
            HISTORY.get(self.curr_index)
        }
    }

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

    pub fn reset_index(&mut self) {
        unsafe {
            self.curr_index = HISTORY.len();
        }
    }
}
