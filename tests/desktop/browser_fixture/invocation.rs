//! Explicit private browser invocation with a loopback-only origin.
use std::env;
use std::io::{self, Write};
use std::path::{Component, Path};

fn command(browser: &str, profile: &str, url: &str, mode: &str, accessibility: bool) -> Result<Vec<String>, String> {
    let binary = Path::new(browser);
    if !binary.is_absolute() || !browser.starts_with("/nix/store/")
        || binary.components().any(|part| part == Component::ParentDir)
    {
        return Err("Explicit immutable browser executable required".into());
    }
    let private = Path::new(profile);
    if !private.is_absolute() || !private.starts_with("/home/fixture")
        || private.components().any(|part| part == Component::ParentDir)
    {
        return Err("Explicit private profile beneath fixture HOME required".into());
    }
    let (scheme, rest) = url.split_once("://").ok_or("Synthetic loopback origin required")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = authority.split_once(':').ok_or("Synthetic loopback origin required")?;
    if !scheme.eq_ignore_ascii_case("http") || host != "127.0.0.1"
        || port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit())
        || port.parse::<u16>().is_err()
    {
        return Err("Synthetic loopback origin required".into());
    }
    if !matches!(mode, "normal" | "app" | "private") {
        return Err("Unsupported explicit browser surface mode".into());
    }
    let mut result = vec![
        browser.to_owned(),
        format!("--user-data-dir={profile}"),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--ozone-platform=wayland".into(),
        "--remote-debugging-pipe".into(),
    ];
    if accessibility {
        result.push("--force-renderer-accessibility".into());
    }
    if mode == "private" {
        result.push("--incognito".into());
    }
    result.push(if mode == "app" { format!("--app={url}") } else { url.to_owned() });
    Ok(result)
}

fn emit(items: &[String]) -> Result<(), String> {
    let mut output = io::stdout().lock();
    for item in items {
        output.write_all(item.as_bytes()).map_err(|error| error.to_string())?;
        output.write_all(b"\0").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["--excluded-setup"] {
        return emit(&["first-run onboarding".into(), "default-browser startup check".into()]);
    }
    if args.len() != 5 {
        return Err("Expected browser, profile, URL, mode and accessibility flag".into());
    }
    let accessibility = match args[4].as_str() {
        "true" => true,
        "false" => false,
        _ => return Err("Accessibility flag must be true or false".into()),
    };
    emit(&command(&args[0], &args[1], &args[2], &args[3], accessibility)?)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_profile_and_loopback_origin_are_required() {
        let browser = "/nix/store/fixture-browser/bin/brave";
        for profile in ["/home/other/profile", "/home/fixture/../other", "relative"] {
            assert!(command(browser, profile, "http://127.0.0.1:8080/page", "normal", false).is_err());
        }
        for origin in ["https://127.0.0.1:8080/", "http://example.invalid:8080/", "http://127.0.0.1:70000/", "http://user@127.0.0.1:8080/"] {
            assert!(command(browser, "/home/fixture/profile", origin, "normal", false).is_err());
        }
        assert!(command("/usr/bin/brave", "/home/fixture/profile", "http://127.0.0.1:8080/", "normal", false).is_err());
    }

    #[test]
    fn explicit_modes_keep_private_flags_and_literal_arguments() {
        let browser = "/nix/store/fixture-browser/bin/brave";
        let profile = "/home/fixture/Literal # profile";
        let url = "http://127.0.0.1:8080/page";
        let ordinary = command(browser, profile, url, "normal", false).unwrap();
        assert_eq!(ordinary[0], browser);
        assert_eq!(ordinary[1], format!("--user-data-dir={profile}"));
        assert_eq!(ordinary.last().unwrap(), url);
        assert!(ordinary.contains(&"--remote-debugging-pipe".into()));
        let app = command(browser, profile, url, "app", true).unwrap();
        assert_eq!(app.last().unwrap(), &format!("--app={url}"));
        assert!(app.contains(&"--force-renderer-accessibility".into()));
        let private = command(browser, profile, url, "private", false).unwrap();
        assert!(private.contains(&"--incognito".into()));
        assert!(command(browser, profile, url, "unknown", false).is_err());
    }
}
