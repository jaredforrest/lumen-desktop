//! Send local folders from desktop entries to the shell's Files IPC.

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

struct Options {
    quickshell: Option<OsString>,
    ipc_command: Option<OsString>,
    shell: Option<OsString>,
    folders: Vec<OsString>,
}

fn parse_args() -> Result<Options, String> {
    let mut input = env::args_os().skip(1);
    let mut options = Options {
        quickshell: None,
        ipc_command: None,
        shell: None,
        folders: Vec::new(),
    };
    let mut folders = false;
    while let Some(arg) = input.next() {
        if folders {
            options.folders.push(arg);
            continue;
        }
        match arg.to_str() {
            Some("--") => folders = true,
            Some("--quickshell") => {
                options.quickshell = Some(input.next().ok_or("--quickshell needs a value")?)
            }
            Some("--ipc-command") => {
                options.ipc_command = Some(input.next().ok_or("--ipc-command needs a value")?)
            }
            Some("--shell") => options.shell = Some(input.next().ok_or("--shell needs a value")?),
            Some(value) if value.starts_with('-') => {
                return Err(format!("unrecognized argument: {value}"));
            }
            _ => {
                folders = true;
                options.folders.push(arg);
            }
        }
    }
    if options.ipc_command.is_none() && (options.quickshell.is_none() || options.shell.is_none()) {
        return Err("--ipc-command or --quickshell with --shell is required".into());
    }
    Ok(options)
}

fn decode_file_url(value: &str) -> Result<PathBuf, String> {
    let mut part = &value[5..];
    if let Some(without_slashes) = part.strip_prefix("//") {
        let (host, path) = without_slashes
            .split_once('/')
            .unwrap_or((without_slashes, ""));
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return Err("remote file URLs are not supported".into());
        }
        part = if path.is_empty() {
            ""
        } else {
            &without_slashes[host.len()..]
        };
    }
    if part.contains(['?', '#']) {
        return Err("file URLs must escape query and fragment characters".into());
    }
    let mut bytes = Vec::with_capacity(part.len());
    let source = part.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            if index + 2 >= source.len() {
                return Err("invalid percent escape in file URL".into());
            }
            let digit = |byte: u8| -> Option<u8> {
                match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    b'A'..=b'F' => Some(byte - b'A' + 10),
                    _ => None,
                }
            };
            let high = digit(source[index + 1]).ok_or("invalid percent escape in file URL")?;
            let low = digit(source[index + 2]).ok_or("invalid percent escape in file URL")?;
            bytes.push(high * 16 + low);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(bytes).map_err(|_| "invalid UTF-8 in file URL")?;
    if decoded.contains('\0') || !Path::new(&decoded).is_absolute() {
        return Err("file URL must contain an absolute path".into());
    }
    Ok(PathBuf::from(decoded))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::RootDir => result.push("/"),
            Component::CurDir => (),
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(value) => result.push(value),
            Component::Prefix(_) => unreachable!("Unix paths have no prefix"),
        }
    }
    Ok(result)
}

fn directory_path(value: &OsStr) -> Result<PathBuf, String> {
    if value.is_empty() {
        return Err("empty or invalid folder argument".into());
    }
    let path = if value
        .to_str()
        .is_some_and(|text| text.to_ascii_lowercase().starts_with("file:"))
    {
        decode_file_url(value.to_str().expect("checked above"))?
    } else {
        let path = PathBuf::from(value);
        if !path.is_dir()
            && value.to_str().is_some_and(|text| {
                text.find(':').is_some_and(|position| {
                    position > 0
                        && text[..position]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                })
            })
        {
            return Err("only local folders and file URLs are supported".into());
        }
        path
    };
    if !path.is_dir() {
        return Err("folder does not exist or is not a directory".into());
    }
    absolute_lexical(&path)
}

fn dispatch(options: Options) -> Result<i32, String> {
    // Validate every argument before sending the first IPC request.
    let paths: Vec<PathBuf> = if options.folders.is_empty() {
        vec![PathBuf::new()]
    } else {
        options
            .folders
            .iter()
            .map(|value| directory_path(value))
            .collect::<Result<_, _>>()?
    };
    for (index, path) in paths.iter().enumerate() {
        let (program, mut args): (&OsStr, Vec<OsString>) =
            if let Some(command) = options.ipc_command.as_deref() {
                (command, vec!["call".into(), "desktop".into()])
            } else {
                (
                    options.quickshell.as_deref().expect("validated arguments"),
                    vec![
                        "-p".into(),
                        options.shell.clone().expect("validated arguments"),
                        "ipc".into(),
                        "call".into(),
                        "desktop".into(),
                    ],
                )
            };
        args.push(if index == 0 { "files" } else { "filesNew" }.into());
        args.push(path.as_os_str().to_os_string());
        let mut child = Command::new(program)
            .args(args)
            .spawn()
            .map_err(|error| error.to_string())?;
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if start.elapsed() >= Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("IPC command timed out".into());
            }
            thread::sleep(Duration::from_millis(10));
        };
        if !status.success() {
            return Ok(status.code().filter(|code| *code > 0).unwrap_or(1));
        }
    }
    Ok(0)
}

fn main() -> ExitCode {
    match parse_args().and_then(dispatch) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("Files could not open folder: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_file_url;
    #[test]
    fn rejects_nonlocal_and_malformed_urls() {
        assert!(decode_file_url("file://server/tmp").is_err());
        assert!(decode_file_url("file:///tmp/%ZZ").is_err());
        assert!(decode_file_url("file:///tmp/%00").is_err());
        assert!(decode_file_url("file:relative").is_err());
        assert!(decode_file_url("file:///tmp/x#fragment").is_err());
    }

    #[test]
    fn decodes_local_urls_once() {
        assert_eq!(
            decode_file_url("file://localhost/tmp/a%20b")
                .unwrap()
                .to_str(),
            Some("/tmp/a b")
        );
        assert_eq!(
            decode_file_url("file:///tmp/%2520").unwrap().to_str(),
            Some("/tmp/%20")
        );
    }
}
