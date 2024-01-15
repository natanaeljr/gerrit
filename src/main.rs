use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::Duration;

use crossterm::cursor::{MoveLeft, MoveToNextLine, MoveUp};
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType, ScrollUp};
use crossterm::{execute, terminal};

use history::HistoryHandle;

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
/// - [ ] Script as input to run automatically commands from a file
/// - [x] HISTORY up/down with on-going command restore on last down-arrow
/// - [ ] Handle left/right arrows and prompt in-middle insert characters,
///       prompt will have to shift the characters.
///
fn main() -> std::io::Result<()> {
    let mut stdout = stdout();
    let mut quit = false;

    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        Print("Gerrit command-line interface"),
        smart_new_line(1)
    )
    .unwrap();

    while !quit {
        print_gerrit_prefix(&mut stdout);

        match read_until_newline(&mut stdout)?.as_str() {
            "help" | "?" => execute!(
                stdout,
                smart_new_line(1),
                Print(" help"),
                smart_new_line(1),
                Print(" remote"),
                smart_new_line(1),
                Print(" quit"),
                smart_new_line(2),
            )
            .unwrap(),
            "remote" => execute!(
                stdout,
                smart_new_line(1),
                Print("remote one"),
                smart_new_line(1),
                Print("remote two"),
                smart_new_line(2),
            )
            .unwrap(),
            "quit" | "exit" => quit = true,
            str if !str.is_empty() => {
                execute!(
                    stdout,
                    smart_new_line(1),
                    PrintStyledContent("x".with(Color::Red)),
                    Print(" Unknown command"),
                    smart_new_line(1)
                )
                .unwrap();
            }
            _ => {
                execute!(stdout, smart_new_line(1)).unwrap();
            }
        };
    }

    execute!(
        stdout,
        smart_new_line(1),
        PrintStyledContent("✓".with(Color::Green)),
        Print(" Done"),
        smart_new_line(1)
    )
    .unwrap();
    terminal::disable_raw_mode()?;
    stdout.flush()?;
    sleep(Duration::from_millis(50));
    Ok(())
}

/// Check if we are at the last row in the terminal,
/// then we may need to scroll up because we are in RAW mode,
/// and the terminal won't do that automatically in this mode.
/// This function quietly does that before `MoveToNextLine`.
/// Then return the new line object, so this function can be used inside
/// execute! or queue! in place of the actual `MoveToNextLine` object.
pub fn smart_new_line(num: u16) -> MoveToNextLine {
    let mut stdout = stdout();
    let curr_row = crossterm::cursor::position().unwrap().1;
    let term_max_row = crossterm::terminal::size().unwrap().1 - 1;
    if curr_row == term_max_row {
        execute!(stdout, ScrollUp(num), MoveUp(num)).unwrap();
    }
    MoveToNextLine(num)
}

pub fn print_gerrit_prefix<W: Write>(stdout: &mut W) {
    execute!(
        stdout,
        Print("gerrit"),
        PrintStyledContent("> ".with(Color::Green)),
    )
    .unwrap();
}

pub fn read_until_newline<W: Write>(stdout: &mut W) -> std::io::Result<String> {
    let mut history = HistoryHandle::get();
    let mut prompt = String::new();
    let mut last_prompt: Option<String> = None;
    loop {
        match event::read() {
            // BACKSPACE
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                modifiers,
                state: _,
            })) => {
                if !prompt.is_empty() {
                    let count: u16;
                    if modifiers == KeyModifiers::ALT {
                        if let Some(idx) = prompt.rfind(" ") {
                            // TODO: fix line wrap and overflow
                            count = (prompt.len() - idx) as u16;
                            _ = prompt.split_off(idx);
                        } else {
                            count = prompt.len() as u16;
                            prompt.clear();
                        }
                    } else {
                        prompt.pop();
                        count = 1;
                    }
                    execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                }
            }
            // ENTER
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if !prompt.is_empty() {
                    history.add(prompt.clone());
                }
                return Ok(prompt);
            }
            // CTRL + C
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^C")).unwrap();
                return Ok(String::from("quit"));
            }
            // CTRL + D
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^D")).unwrap();
                return Ok(String::from("quit"));
            }
            // CTRL + L
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('l'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                let curr_row = crossterm::cursor::position().unwrap().1;
                execute!(stdout, ScrollUp(curr_row), MoveUp(curr_row)).unwrap()
            }
            // ARROW UP
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if let Some(up_next) = history.up_next() {
                    let count = prompt.len() as u16;
                    if last_prompt == None {
                        last_prompt = Some(prompt.clone())
                    }
                    prompt = up_next.clone();
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                }
            }
            // ARROW DOWN
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                if let Some(down_next) = history.down_next() {
                    let count = prompt.len() as u16;
                    prompt = down_next.clone();
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine)).unwrap();
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                } else {
                    let count = prompt.len() as u16;
                    if count > 0 {
                        execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine),).unwrap();
                    }
                    if last_prompt.is_some() {
                        prompt = last_prompt.unwrap();
                        last_prompt = None;
                    }
                    execute!(stdout, Print(prompt.as_str())).unwrap();
                }
            }
            // CHARACTERS
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                execute!(stdout, Print(c)).unwrap();
                prompt.push(c);
            }
            // ANYTHING
            _ => {}
        }
    }
}
