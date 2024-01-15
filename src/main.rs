use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::Duration;

use crossterm::cursor::{MoveLeft, MoveToNextLine, MoveUp};
use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Print, PrintStyledContent, Stylize};
use crossterm::terminal::{Clear, ClearType, ScrollUp};
use crossterm::{execute, terminal};

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
/// - [ ] Handle scroll when cursor is at last row of the terminal window
/// - [ ] Command History
/// - [ ] script as input to run automatically commands from a file

fn main() -> std::io::Result<()> {
    terminal::enable_raw_mode()?;

    let mut stdout = stdout();
    let mut quit = false;

    execute!(
        stdout,
        Print("Gerrit command-line interface"),
        smart_new_line(1)
    );

    while !quit {
        print_gerrit_prefix(&mut stdout);

        match read_until_newline(&mut stdout)?.as_str() {
            "help" => execute!(
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
            "quit" => quit = true,
            str if !str.is_empty() => {
                execute!(
                    stdout,
                    smart_new_line(1),
                    PrintStyledContent("x".with(Color::Red)),
                    Print(" Unknown command"),
                    smart_new_line(1)
                );
            }
            _ => {
                execute!(stdout, smart_new_line(1));
            }
        };
    }

    execute!(
        stdout,
        smart_new_line(1),
        PrintStyledContent("✓".with(Color::Green)),
        Print(" Done"),
        smart_new_line(1)
    );
    terminal::disable_raw_mode()?;
    stdout.flush()?;
    sleep(Duration::from_millis(50));
    Ok(())
}

pub fn smart_new_line(num: u16) -> MoveToNextLine {
    let mut stdout = stdout();
    let cursor_row = crossterm::cursor::position().unwrap().1;
    let term_row = crossterm::terminal::size().unwrap().1 - 1;
    if cursor_row == term_row {
        execute!(stdout, ScrollUp(num), MoveUp(num));
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
    let mut string = String::new();
    loop {
        match event::read() {
            // backspace
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                modifiers,
                state: _,
            })) => {
                if !string.is_empty() {
                    let mut count: u16 = 0;
                    if modifiers == KeyModifiers::ALT {
                        if let Some(idx) = string.rfind(" ") {
                            // TODO: fix line wrap and overflow
                            count = (string.len() - idx) as u16;
                            _ = string.split_off(idx);
                        } else {
                            count = string.len() as u16;
                            string.clear();
                        }
                    } else {
                        string.pop();
                        count = 1;
                    }
                    execute!(stdout, MoveLeft(count), Clear(ClearType::UntilNewLine));
                }
            }
            // enter
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => return Ok(string),
            // ctrl + c
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^C"));
                return Ok(String::from("quit"));
            }
            // ctrl + d
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                execute!(stdout, Print("^D"));
                return Ok(String::from("quit"));
            }
            // ctrl + l
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char('l'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                state: _,
            })) => {
                let curr_row = crossterm::cursor::position().unwrap().1;
                execute!(stdout, ScrollUp(curr_row), MoveUp(curr_row)).unwrap()
            }
            // characters
            Ok(Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                modifiers: _,
                state: _,
            })) => {
                execute!(stdout, Print(c));
                string.push(c)
            }
            // anything
            _ => {}
        }
    }
}
