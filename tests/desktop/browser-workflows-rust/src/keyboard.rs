//! Stable physical US evdev keyboard events for isolated native shortcuts.
use crate::{Result, fail};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

pub type Event = (&'static str, u32, u32);
fn modifier(name: &str) -> Option<u32> {
    Some(match name {
        "shift" => 1,
        "ctrl" => 4,
        "alt" => 8,
        "logo" => 64,
        _ => return None,
    })
}
pub fn special(name: &str) -> Option<u32> {
    Some(match name {
        "Escape" => 1,
        "Return" => 28,
        "Tab" => 15,
        "BackSpace" => 14,
        "Delete" => 111,
        "Left" => 105,
        "Right" => 106,
        "Up" => 103,
        "Down" => 108,
        "Home" => 102,
        "End" => 107,
        "F11" => 87,
        "F12" => 88,
        "space" => 57,
        "minus" => 12,
        _ => return None,
    })
}
fn plain(character: char) -> Option<u32> {
    let code = match character {
        '1'..='9' => (character as u32 - '1' as u32) + 2,
        '0' => 11,
        '-' => 12,
        '=' => 13,
        '[' => 26,
        ']' => 27,
        ';' => 39,
        '\'' => 40,
        '`' => 41,
        '\\' => 43,
        ',' => 51,
        '.' => 52,
        '/' => 53,
        ' ' => 57,
        _ => {
            for (letters, start) in [("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)] {
                if let Some(index) = letters.find(character) {
                    return Some(start + index as u32);
                }
            }
            return None;
        }
    };
    Some(code)
}
fn character_event(character: char) -> Result<(u32, u32)> {
    if let Some(code) = plain(character) {
        return Ok((code, 0));
    }
    let shifted = "!@#$%^&*()_+{}:\"~|<>?";
    let underlying = "1234567890-=[];'`\\,./";
    if let Some(index) = shifted.find(character) {
        return Ok((
            plain(underlying.chars().nth(index).ok_or("shift map")?).ok_or("shift code")?,
            1,
        ));
    }
    if character.is_ascii_uppercase() {
        return Ok((
            plain(character.to_ascii_lowercase()).ok_or("letter code")?,
            1,
        ));
    }
    Err(fail(
        "Use the private clipboard for non-US text; no layout guessing",
    ))
}
pub fn text_events(value: &str) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for character in value.chars() {
        let (code, mods) = character_event(character)?;
        events.extend([("m", mods, 0), ("k", code, 1), ("k", code, 0)]);
    }
    events.push(("m", 0, 0));
    Ok(events)
}
fn ready(fd: i32) -> Result<()> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, 3000) };
    if result <= 0 {
        return Err(fail(
            "Stable private keyboard did not acknowledge exact command",
        ));
    }
    Ok(())
}
pub struct PhysicalKeyboard {
    process: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    sequence: u64,
    modifiers: u32,
}
impl PhysicalKeyboard {
    pub fn start(
        binary: &Path,
        environment: &BTreeMap<String, String>,
        log: &Path,
    ) -> Result<Self> {
        let mut command = Command::new(binary);
        let logfile = File::create(log)?;
        command
            .env_clear()
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(logfile));
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut process = command.spawn()?;
        let stdin = process.stdin.take().ok_or("keyboard stdin")?;
        let stdout = process.stdout.take().ok_or("keyboard stdout")?;
        let mut keyboard = Self {
            process,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            sequence: 0,
            modifiers: 0,
        };
        let line = keyboard.read_line()?;
        if line.trim() != "ready" {
            return Err(fail("Stable private keyboard did not become ready"));
        }
        Ok(keyboard)
    }
    fn read_line(&mut self) -> Result<String> {
        ready(self.stdout.get_ref().as_raw_fd())?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(line)
    }
    pub fn send(&mut self, operation: &str, value: u32, state: u32) -> Result<()> {
        self.sequence += 1;
        let sequence = self.sequence;
        writeln!(
            self.stdin.as_mut().ok_or("keyboard closed")?,
            "{operation} {sequence} {value} {state}"
        )?;
        self.stdin.as_mut().unwrap().flush()?;
        if self.read_line()?.trim() != format!("ack {sequence}") {
            return Err(fail(
                "Stable private keyboard did not acknowledge exact command",
            ));
        }
        Ok(())
    }
    pub fn type_text(&mut self, value: &str) -> Result<()> {
        for (operation, code, state) in text_events(value)? {
            self.send(operation, code, state)?;
        }
        Ok(())
    }
    pub fn key(&mut self, values: &[&str]) -> Result<()> {
        if values.len() == 1 {
            return self.type_text(values[0]);
        }
        let mut chunks = values.chunks_exact(2);
        for item in &mut chunks {
            match item[0] {
                "-M" | "-m" => {
                    let bit = modifier(item[1]).ok_or("Unsupported modifier")?;
                    if item[0] == "-M" {
                        self.modifiers |= bit;
                    } else {
                        self.modifiers &= !bit;
                    }
                    self.send("m", self.modifiers, 0)?;
                }
                "-k" => {
                    let code = special(item[1])
                        .or_else(|| {
                            item[1]
                                .chars()
                                .next()
                                .filter(|_| item[1].chars().count() == 1)
                                .and_then(plain)
                        })
                        .ok_or("Unsupported named physical key")?;
                    self.send("k", code, 1)?;
                    self.send("k", code, 0)?;
                }
                _ => return Err(fail("Unsupported structured keyboard operation")),
            }
        }
        if !chunks.remainder().is_empty() {
            return Err(fail("Incomplete keyboard operation"));
        }
        std::thread::sleep(Duration::from_millis(120));
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        self.stdin.take();
        let end = Instant::now() + Duration::from_secs(3);
        while self.process.try_wait()?.is_none() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.process.try_wait()?.is_none() {
            unsafe {
                libc::killpg(self.process.id() as i32, libc::SIGKILL);
            }
            let _ = self.process.wait();
        }
        Ok(())
    }
}
impl Drop for PhysicalKeyboard {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
