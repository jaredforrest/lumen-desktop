//! London solar appearance and declarative desktop preferences.
use chrono::{DateTime, Datelike, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::London;
use serde_json::{Value, json};
use std::{env, fs, fs::OpenOptions, io::Write, os::fd::AsRawFd, path::{Path, PathBuf}, process::Command, time::{SystemTime, UNIX_EPOCH}};

const PALETTE: &str = include_str!("../../palette.json");
const LAUNCHER: &str = include_str!("../../launcher.rasi.in");
const ACCENTS: &[(&str, &str, &str)] = &[
    ("blue", "#007aff", "#0a84ff"), ("purple", "#a855c7", "#bf5af2"),
    ("pink", "#d94382", "#ff6482"), ("red", "#df3038", "#ff453a"),
    ("orange", "#b65c00", "#ff9f0a"), ("yellow", "#947500", "#ffd60a"),
    ("green", "#248a3d", "#30d158"), ("graphite", "#707078", "#98989f"),
];

#[derive(Default)]
struct Args {
    mode: Option<String>, at: Option<String>, state_dir: Option<PathBuf>,
    print_only: bool, no_desktop: bool, settings: bool, set: Option<(String, String)>,
    set_search: Option<(String, String)>, set_notifications: Option<(String, String)>,
    set_control_centre: Option<(String, String)>, set_windows: Option<(String, String)>,
    set_desktop: Option<(String, String)>, set_menu_bar: Option<(String, String)>,
}

fn arguments() -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut items = env::args().skip(1);
    while let Some(arg) = items.next() {
        match arg.as_str() {
            "--mode" => parsed.mode = Some(items.next().ok_or("--mode requires a value")?),
            "--at" => parsed.at = Some(items.next().ok_or("--at requires a value")?),
            "--state-dir" => parsed.state_dir = Some(PathBuf::from(items.next().ok_or("--state-dir requires a path")?)),
            "--print" => parsed.print_only = true,
            "--no-desktop" => parsed.no_desktop = true,
            "--settings" => parsed.settings = true,
            "--set" => parsed.set = Some((items.next().ok_or("--set requires a key")?, items.next().ok_or("--set requires JSON")?)),
            "--set-search" => parsed.set_search = Some((items.next().ok_or("--set-search requires a key")?, items.next().ok_or("--set-search requires JSON")?)),
            "--set-notifications" => parsed.set_notifications = Some((items.next().ok_or("--set-notifications requires a key")?, items.next().ok_or("--set-notifications requires JSON")?)),
            "--set-control-centre" => parsed.set_control_centre = Some((items.next().ok_or("--set-control-centre requires a key")?, items.next().ok_or("--set-control-centre requires JSON")?)),
            "--set-windows" => parsed.set_windows = Some((items.next().ok_or("--set-windows requires a key")?, items.next().ok_or("--set-windows requires JSON")?)),
            "--set-desktop" => parsed.set_desktop = Some((items.next().ok_or("--set-desktop requires a key")?, items.next().ok_or("--set-desktop requires JSON")?)),
            "--set-menu-bar" => parsed.set_menu_bar = Some((items.next().ok_or("--set-menu-bar requires a key")?, items.next().ok_or("--set-menu-bar requires JSON")?)),
            _ => return Err(format!("Unknown option: {arg}")),
        }
    }
    if parsed.mode.as_deref().is_some_and(|mode| !["auto", "light", "dark"].contains(&mode)) {
        return Err("Unsupported appearance mode".into());
    }
    Ok(parsed)
}

fn valid(key: &str, value: &Value) -> bool {
    match key {
        "textScale" => value.as_f64().is_some_and(|v| [1.0, 1.15, 1.3, 1.5].contains(&v)),
        "mode" => value.as_str().is_some_and(|v| ["auto", "light", "dark"].contains(&v)),
        "iconAppearance" => value.as_str().is_some_and(|v| ["default", "auto", "dark"].contains(&v)),
        "accent" => value.as_str().is_some_and(|v| ACCENTS.iter().any(|(name, _, _)| *name == v)),
        "switchToAppSpace" | "dockMagnification" | "reduceMotion" | "reduceTransparency" |
        "increaseContrast" | "dockRecentApps" | "dockIndicators" | "dockAutoHide" => value.is_boolean(),
        "scrollbars" => value.as_str().is_some_and(|v| ["auto", "when_scrolling", "always"].contains(&v)),
        "sidebarIconSize" => value.as_str().is_some_and(|v| ["small", "medium", "large"].contains(&v)),
        "dockPosition" => value.as_str().is_some_and(|v| ["bottom", "left", "right"].contains(&v)),
        "dockSize" => value.as_u64().is_some_and(|v| (32..=72).contains(&v)),
        "dockPins" => value.is_null() || value.as_array().is_some_and(|rows| {
            let mut seen = std::collections::HashSet::new();
            rows.iter().all(|row| row.as_str().is_some_and(|id| !id.is_empty() && id.len() <= 256 &&
                id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) && seen.insert(id)))
        }),
        "dockFiles" => value.as_array().is_some_and(|rows| {
            let mut seen = std::collections::HashSet::new();
            rows.iter().all(|row| row.as_str().is_some_and(|path| local_path(path) && seen.insert(path)))
        }),
        "dockFolders" => value.as_array().is_some_and(|rows| {
            let mut seen = std::collections::HashSet::new();
            rows.iter().all(|row| row.as_object().is_some_and(|folder| {
                folder.keys().all(|key| ["path", "sort", "descending", "presentation"].contains(&key.as_str())) &&
                folder.get("path").and_then(Value::as_str).is_some_and(|path| local_path(path) && seen.insert(path)) &&
                folder.get("sort").map(Value::as_str).unwrap_or(Some("modified")).is_some_and(|v| ["name", "modified", "size", "type"].contains(&v)) &&
                folder.get("descending").map(Value::as_bool).unwrap_or(Some(true)).is_some() &&
                folder.get("presentation").map(Value::as_str).unwrap_or(Some("auto")).is_some_and(|v| ["list", "grid", "auto"].contains(&v))
            }))
        }),
        _ => false,
    }
}

fn local_path(path: &str) -> bool {
    path.starts_with('/') && path.len() <= 4096 && !path.chars().any(|c| (c as u32) < 32)
}

fn valid_option(section: &str, key: &str, value: &Value) -> bool {
    match section {
        "appearance" => valid(key, value),
        "desktop" => match key {
            "showFiles" | "fileDescending" => value.is_boolean(),
            "fileSort" => value.as_str().is_some_and(|v| ["name", "modified", "size", "kind"].contains(&v)),
            _ => false,
        },
        "windows" => match key {
            "tileMargins" | "edgeTiling" | "optionTiling" | "topEdgeFill" | "topEdgeOverview" => value.is_boolean(),
            "tileGap" => value.as_u64().is_some_and(|v| v <= 32),
            "titlebarDoubleClick" => value.as_str().is_some_and(|v| ["fill", "zoom", "minimize", "none"].contains(&v)),
            _ => false,
        },
        "search" => match key {
            "queryHistory" => value.is_boolean(),
            "disabledProviders" => unique_names(value, &["applications", "files", "actions", "clipboard"], false),
            _ => false,
        },
        "controlCentre" => match key {
            "order" => unique_names(value, &["wifi", "bluetooth", "sharing", "media", "displays", "appearance", "focus", "brightness", "sound"], true),
            "hidden" => unique_names(value, &["wifi", "bluetooth", "sharing", "media", "displays", "appearance", "focus", "brightness", "sound"], false),
            _ => false,
        },
        "notifications" => key == "preview" && value.as_str().is_some_and(|v| ["always", "never"].contains(&v)),
        "menuBar" => match key {
            "order" => unique_names(value, &["battery", "sound", "focus", "search", "controls"], true),
            "visibility" => value.as_str().is_some_and(|v| ["always", "desktop", "fullscreen", "auto_hide"].contains(&v)),
            "fullscreenVisibility" => value.as_str().is_some_and(|v| ["automatic", "always"].contains(&v)),
            "hourCycle" => value.as_str().is_some_and(|v| ["locale", "12", "24"].contains(&v)),
            "locale" => value.as_str().is_some_and(|v| locale(v)),
            "showDate" | "showWeekday" | "showSeconds" => value.is_boolean(),
            _ => false,
        },
        _ => false,
    }
}

fn unique_names(value: &Value, allowed: &[&str], exact: bool) -> bool {
    let Some(rows) = value.as_array() else { return false; };
    let mut seen = std::collections::HashSet::new();
    rows.iter().all(|row| row.as_str().is_some_and(|name| allowed.contains(&name) && seen.insert(name))) &&
        (!exact || rows.len() == allowed.len())
}

fn locale(value: &str) -> bool {
    if value.is_empty() { return true; }
    let (language, country) = value.split_once('_').unwrap_or((value, ""));
    (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase()) &&
        (!value.contains('_') || country.len() == 2 && country.bytes().all(|b| b.is_ascii_uppercase()))
}

fn luminance(color: &str) -> f64 {
    let channel = |index| u8::from_str_radix(&color[index..index + 2], 16).unwrap_or(0) as f64 / 255.0;
    [1, 3, 5].iter().zip([0.2126, 0.7152, 0.0722]).map(|(&index, weight)| {
        let value = channel(index);
        (if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }) * weight
    }).sum()
}

fn contrast(first: &str, second: &str) -> f64 {
    let (a, b) = (luminance(first), luminance(second));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn resolve_palette(palettes: &Value, mode: &str, prefs: &Value) -> Result<Value, String> {
    let mut colors = palettes[mode].clone();
    let accent = prefs["accent"].as_str().ok_or("Invalid accent")?;
    let (_, light, dark) = ACCENTS.iter().find(|(name, _, _)| *name == accent).ok_or("Invalid accent")?;
    colors["accent"] = json!(if mode == "dark" { dark } else { light });
    colors["error"] = json!(if mode == "dark" { "#ffaba5" } else { "#b42318" });
    colors["success"] = json!(if mode == "dark" { "#55d788" } else { "#196633" });
    colors["warning"] = json!(if mode == "dark" { "#e0b85a" } else { "#775000" });
    colors["destructiveFill"] = json!("#b42318");
    colors["destructiveText"] = json!("#ffffff");
    if mode == "dark" && ["orange", "yellow", "green", "graphite"].contains(&accent) {
        colors["selectedText"] = json!("#161618");
    }
    if prefs["increaseContrast"] == true {
        colors["text"] = json!(if mode == "dark" { "#ffffff" } else { "#000000" });
        colors["muted"] = json!(if mode == "dark" { "#e0e0e0" } else { "#393939" });
        colors["border"] = json!(if mode == "dark" { "#a8a8ac" } else { "#626266" });
    }
    let selected = colors["selectedText"].as_str().ok_or("Invalid selected text")?;
    let fill = colors["accent"].as_str().ok_or("Invalid accent colour")?;
    if contrast(selected, fill) < 4.5 {
        colors["selectedText"] = json!(if contrast("#ffffff", fill) >= contrast("#000000", fill) { "#ffffff" } else { "#000000" });
    }
    Ok(colors)
}

fn settings(args: &[&str]) -> Result<Value, String> {
    let output = Command::new("lumen-nix-settings").args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let message = serde_json::from_slice::<Value>(&output.stdout).ok()
            .and_then(|value| value["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stdout).trim().to_string());
        return Err(message);
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn parsed_time(value: Option<&str>) -> Result<DateTime<chrono_tz::Tz>, String> {
    let Some(value) = value else { return Ok(Utc::now().with_timezone(&London)); };
    if let Ok(date) = DateTime::parse_from_rfc3339(value) { return Ok(date.with_timezone(&London)); }
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M"))
        .map_err(|e| format!("Invalid date: {e}"))?;
    match London.from_local_datetime(&naive) {
        LocalResult::Single(date) | LocalResult::Ambiguous(date, _) => Ok(date),
        LocalResult::None => Err("Date is outside London local time".into()),
    }
}

fn solar_minutes(day: u32) -> (f64, f64) {
    // NOAA equation of time and apparent solar altitude at 0.833 degrees.
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (day as f64 - 1.0);
    let eqtime = 229.18 * (0.000075 + 0.001868 * gamma.cos() - 0.032077 * gamma.sin()
        - 0.014615 * (2.0 * gamma).cos() - 0.040849 * (2.0 * gamma).sin());
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos() + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos() + 0.00148 * (3.0 * gamma).sin();
    let latitude = 51.5074_f64.to_radians();
    let hour_angle = ((90.833_f64.to_radians().cos() / (latitude.cos() * decl.cos()))
        - latitude.tan() * decl.tan()).acos().to_degrees();
    let noon = 720.0 - 4.0 * -0.1278 - eqtime;
    (noon - 4.0 * hour_angle, noon + 4.0 * hour_angle)
}

fn solar(now: DateTime<chrono_tz::Tz>) -> (String, String, String) {
    let date = now.date_naive();
    let midnight = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let (rise_min, set_min) = solar_minutes(date.ordinal());
    let rise = midnight + chrono::Duration::seconds((rise_min * 60.0).round() as i64);
    let set = midnight + chrono::Duration::seconds((set_min * 60.0).round() as i64);
    let mode = if now.with_timezone(&Utc) >= rise && now.with_timezone(&Utc) < set { "light" } else { "dark" };
    (mode.into(), rise.with_timezone(&London).to_rfc3339(), set.with_timezone(&London).to_rfc3339())
}

fn system_info() -> Value {
    let mut name = "NixOS".to_string();
    if let Ok(contents) = fs::read_to_string("/etc/os-release") {
        if let Some(value) = contents.lines().find_map(|line| line.strip_prefix("PRETTY_NAME=")) {
            name = value.trim_matches('"').to_string();
        }
    }
    json!({"system":name,"desktop":if env::var_os("SWAYSOCK").is_some() { "Sway" } else { "Wayland" }})
}

fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) { return Ok(()); }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{nonce}", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
    file.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program).args(args).output().is_ok_and(|result| result.status.success())
}

fn update(section: &str, field: &Option<(String, String)>, current: &mut Value, error: &str) -> Result<(), String> {
    let Some((key, raw)) = field else { return Ok(()); };
    let value: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    if !valid_option(section, key, &value) { return Err(error.into()); }
    let mut patch = serde_json::Map::new();
    patch.insert(key.clone(), value);
    let encoded = Value::Object(patch).to_string();
    *current = settings(&["patch", section, &encoded])?[section].clone();
    Ok(())
}

fn output(prefs: &Value, search: &Value, notifications: &Value, control: &Value,
    menu: &Value, windows: &Value, desktop: &Value, appearance: &Value, warning: Option<String>) -> Value {
    let mut result = json!({"preferences":prefs,"searchPreferences":search,
        "notificationsPreferences":notifications,"controlCentrePreferences":control,
        "menuBarPreferences":menu,"windowsPreferences":windows,
        "desktopPreferences":desktop,"appearance":appearance,"system":system_info()});
    if let Some(warning) = warning { result["warning"] = json!(warning); }
    result
}

fn enabled(value: &Value, key: &str) -> bool { value.get(key).and_then(Value::as_bool).unwrap_or(true) }
fn placement_commands(preferences: &Value, preferred_zoom: bool) -> Vec<String> {
    let mut commands = vec![format!("lumen_window margins {}", if enabled(preferences,"tileMargins") { "enable" } else { "disable" })];
    let action = preferences["titlebarDoubleClick"].as_str().unwrap_or("fill");
    if action != "zoom" || preferred_zoom { commands.push(format!("lumen_window titlebar-action {action}")); }
    commands
}

fn execute() -> Result<(), String> {
    let args = arguments()?;
    let state = args.state_dir.unwrap_or_else(|| {
        env::var_os("XDG_STATE_HOME").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
            .join("lumen-desktop")
    });
    fs::create_dir_all(&state).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new().create(true).append(true).open(state.join(".appearance.lock")).map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err("Cannot lock appearance state".into()); }
    let all = settings(&["read"])?;
    let mut prefs = all["appearance"].clone();
    let mut search = all.get("search").cloned().unwrap_or(json!({"disabledProviders":[]}));
    let mut notifications = all.get("notifications").cloned().unwrap_or(json!({"preview":"always","policies":{}}));
    let mut control = all.get("controlCentre").cloned().unwrap_or(json!({"order":["wifi","bluetooth","sharing","media","displays","appearance","focus","brightness","sound"],"hidden":[]}));
    let mut menu = all.get("menuBar").cloned().unwrap_or(json!({"order":["battery","sound","focus","search","controls"],"hourCycle":"locale","locale":"","visibility":"desktop","showDate":true,"showWeekday":true,"showSeconds":false}));
    let mut windows = all.get("windows").cloned().unwrap_or(json!({"tileMargins":true,"tileGap":8,"titlebarDoubleClick":"fill","edgeTiling":true,"optionTiling":true,"topEdgeFill":true,"topEdgeOverview":true}));
    let mut desktop = all.get("desktop").cloned().unwrap_or(json!({"showFiles":false,"fileSort":"name","fileDescending":false}));
    update("menuBar", &args.set_menu_bar, &mut menu, "Unsupported menu bar preference or value")?;
    update("desktop", &args.set_desktop, &mut desktop, "Unsupported desktop preference or value")?;
    update("windows", &args.set_windows, &mut windows, "Unsupported window preference or value")?;
    update("controlCentre", &args.set_control_centre, &mut control, "Unsupported control layout preference or value")?;
    update("notifications", &args.set_notifications, &mut notifications, "Unsupported notification preference or value")?;
    update("search", &args.set_search, &mut search, "Unsupported search preference or value")?;
    if args.set_search.as_ref().is_some_and(|(key, raw)| key == "queryHistory" && serde_json::from_str::<Value>(raw).ok() == Some(Value::Bool(false))) {
        use std::process::Stdio;
        let mut child = Command::new("timeout").args(["10s", "lumen-search-history"])
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        child.stdin.take().ok_or("Search history stdin unavailable")?.write_all(b"{\"action\":\"clear\"}\n").map_err(|e| e.to_string())?;
        if !child.wait().map_err(|e| e.to_string())?.success() { return Err("Could not clear search history".into()); }
    }
    update("appearance", &args.set, &mut prefs, "Unsupported preference or value")?;
    let now = parsed_time(args.at.as_deref())?;
    let (solar_mode, sunrise, sunset) = solar(now);
    let source = args.mode.as_deref().or_else(|| prefs["mode"].as_str()).unwrap_or("auto");
    let mode = if source == "auto" { solar_mode.as_str() } else { source };
    let palettes: Value = serde_json::from_str(PALETTE).map_err(|e| e.to_string())?;
    let colors = resolve_palette(&palettes, mode, &prefs)?;
    let data = json!({"mode":mode,"source":source,"location":"London","timezone":"Europe/London",
        "font":palettes["font"],"palette":colors,"preferences":prefs,"searchPreferences":search,
        "notificationsPreferences":notifications,"controlCentrePreferences":control,
        "menuBarPreferences":menu,"windowsPreferences":windows,"desktopPreferences":desktop,
        "sunrise":sunrise,"sunset":sunset});
    if args.print_only { println!("{data}"); return Ok(()); }
    let editing = args.set.is_some() || args.set_search.is_some() || args.set_notifications.is_some() ||
        args.set_control_centre.is_some() || args.set_windows.is_some() || args.set_desktop.is_some() || args.set_menu_bar.is_some();
    if args.settings && !editing {
        println!("{}", output(&prefs,&search,&notifications,&control,&menu,&windows,&desktop,&data,None));
        return Ok(());
    }
    let previous: Value = fs::read(state.join("appearance.json")).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
    let mut launcher = LAUNCHER.to_string();
    for (key, value) in colors.as_object().ok_or("Invalid palette")? {
        launcher = launcher.replace(&format!("@{key}@"), value.as_str().ok_or("Invalid colour")?);
    }
    launcher = launcher.replace("@font@", palettes["font"].as_str().ok_or("Invalid font")?);
    atomic_write(&state.join("launcher.rasi"), &launcher)?;
    atomic_write(&state.join("appearance.json"), &format!("{}\n", serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?))?;
    let mut warnings = Vec::new();
    if !args.no_desktop {
        let capabilities: Value = if env::var_os("SWAYSOCK").is_some() {
            Command::new("timeout").args(["3s","swaymsg","-t","get_version","-r"]).output().ok()
                .filter(|result| result.status.success())
                .and_then(|result| serde_json::from_slice(&result.stdout).ok()).unwrap_or(Value::Null)
        } else { Value::Null };
        let native_frame = capabilities["native_window_controls"].as_bool().unwrap_or(false);
        let compositor_effects = capabilities["compositor_effects"].as_bool().unwrap_or(false);
        let unified_headers = capabilities["native_window_actions"].as_bool().unwrap_or(false);
        let placement_preferences = capabilities["window_placement_preferences"].as_bool().unwrap_or(false);
        let edge_preferences = capabilities["native_edge_tiling"] == true;
        let preferred_zoom = capabilities["native_preferred_zoom"] == true;
        let layout = if native_frame { ":" } else { "close,minimize,maximize:" };
        if !run("gsettings", &["set","org.gnome.desktop.wm.preferences","button-layout",layout]) {
            warnings.push("Could not update window-control ownership".into());
        }
        if native_frame {
            for state in ["focused","focused_inactive","unfocused"] {
                let foreground = colors[if state == "focused" { "text" } else { "muted" }].as_str().unwrap_or("");
                if !run("swaymsg", &[&format!("client.{state}"), colors["border"].as_str().unwrap_or(""),
                    colors["background"].as_str().unwrap_or(""),foreground,
                    colors["accent"].as_str().unwrap_or(""), colors["border"].as_str().unwrap_or("")]) {
                    warnings.push("Could not update window-frame colours".into());
                }
            }
        }
        if args.set_windows.is_some() && !placement_preferences {
            warnings.push("Window placement preferences require the updated compositor at next login".into());
        }
        if placement_preferences {
            if windows["titlebarDoubleClick"] == "zoom" && !preferred_zoom {
                warnings.push("Preferred-size zoom requires the updated compositor at next login".into());
            }
            for command in placement_commands(&windows, preferred_zoom) {
                if !run("swaymsg", &[&command]) { warnings.push("Could not apply window placement preference".into()); }
            }
        }
        if edge_preferences {
            let mut commands = vec![format!("lumen_window tile-gap {}", windows["tileGap"].as_u64().unwrap_or(8))];
            for (key,setting) in [("edgeTiling","edge-tiling"),("optionTiling","option-tiling"),
                ("topEdgeFill","top-edge-fill"),("topEdgeOverview","top-edge-overview")] {
                commands.push(format!("lumen_window {setting} {}", if enabled(&windows,key) { "enable" } else { "disable" }));
            }
            for command in commands {
                if !run("swaymsg", &[&command]) { warnings.push("Could not apply window drag preference".into()); }
            }
        } else if args.set_windows.as_ref().is_some_and(|(key,_)| ["tileGap","edgeTiling","optionTiling","topEdgeFill","topEdgeOverview"].contains(&key.as_str())) {
            warnings.push("Window drag preferences require the updated compositor at next login".into());
        }
        if unified_headers && !run("swaymsg", &["[app_id=\"org.quickshell\" title=\"^(Files.*|System Settings|Calendar|Text Tools|Window Overview|Quick Look|File Info|Desktop Help|Session Options|Remote Desktop)$\"] border pixel 0"]) {
            warnings.push("Could not apply unified app headers".into());
        }
        if compositor_effects {
            let commands = vec!["corner_radius 12".into(),"smart_corner_radius disable".into(),"shadows enable".into(),
                "shadows_on_csd enable".into(),"shadow_blur_radius 28".into(),"shadow_color #00000060".into(),
                "shadow_inactive_color #00000030".into(),"shadow_offset 0 6".into(),"blur enable".into(),
                "blur_xray disable".into(),"blur_passes 3".into(),"blur_radius 5".into(),
                format!("animation_duration_ms {}", if prefs["reduceMotion"] == true { "0" } else { "160" }),
                "layer_effects \"lumen-dock\" \"blur enable; corner_radius 22\"".into(),
                "layer_effects \"lumen-menu\" \"blur enable\"".into(),
                "layer_effects \"lumen-menu-popup\" \"blur enable; blur_ignore_transparent enable\"".into()];
            for command in commands {
                if !run("swaymsg", &[&command]) {
                    warnings.push(format!("Could not apply compositor appearance: {}", command.split_whitespace().next().unwrap_or("")));
                }
            }
        }
        for (key, value) in [("color-scheme", format!("prefer-{mode}")),
            ("gtk-theme", if mode == "dark" { "WhiteSur-Dark".into() } else { "WhiteSur-Light".into() })] {
            if !run("gsettings", &["set", "org.gnome.desktop.interface", key, &value]) {
                warnings.push(format!("Could not update application {key}"));
            }
        }
    }
    if editing {
        println!("{}", output(&prefs,&search,&notifications,&control,&menu,&windows,&desktop,&data,Some(warnings.join("; "))));
    } else if previous["mode"] != mode {
        println!("Appearance: {mode} London sunset: {sunset}");
    }
    Ok(())
}

fn main() {
    let probe: Vec<String> = env::args().skip(1).collect();
    if probe.first().is_some_and(|item| item == "--resolve-palette") {
        let result = (|| {
            if probe.len() != 4 || !["light","dark"].contains(&probe[1].as_str()) || !["true","false"].contains(&probe[3].as_str()) {
                return Err("Expected mode, accent and contrast flag".to_string());
            }
            let palettes: Value = serde_json::from_str(PALETTE).map_err(|e| e.to_string())?;
            resolve_palette(&palettes, &probe[1], &json!({"accent":probe[2],"increaseContrast":probe[3] == "true"}))
        })();
        match result {
            Ok(colors) => println!("{colors}"),
            Err(error) => { println!("{}",json!({"error":error})); std::process::exit(1); }
        }
        return;
    }
    if let Err(error) = execute() {
        println!("{}", json!({"error":error}));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn london_solar_day_and_night() {
        let noon = parsed_time(Some("2026-06-21T12:00:00+01:00")).unwrap();
        let midnight = parsed_time(Some("2026-06-21T00:00:00+01:00")).unwrap();
        assert_eq!(solar(noon).0, "light");
        assert_eq!(solar(midnight).0, "dark");
        assert!(solar(noon).1.contains("+01:00"));
        // Astral's London 2026 solstice times are 04:43:27 and 21:21:12.
        let rise = DateTime::parse_from_rfc3339(&solar(noon).1).unwrap();
        let set = DateTime::parse_from_rfc3339(&solar(noon).2).unwrap();
        assert!((rise.timestamp() - 1_782_013_407).abs() < 90);
        assert!((set.timestamp() - 1_782_073_272).abs() < 90);
    }
    #[test]
    fn rejects_invalid_preferences() {
        assert!(!valid("mode", &json!("invalid")));
        assert!(!valid("dockMagnification", &json!(1)));
        assert!(valid("accent", &json!("graphite")));
    }
    #[test]
    fn validates_extended_preferences_and_layouts() {
        for value in [json!(1),json!(1.15),json!(1.3),json!(1.5)] { assert!(valid("textScale",&value)); }
        for value in [json!(true),json!(2),json!(1.25),json!("1.5")] { assert!(!valid("textScale",&value)); }
        assert!(valid("dockPins",&json!(["brave-browser","org.gnome.Evince.desktop"])));
        assert!(!valid("dockPins",&json!(["same","same"])));
        assert!(valid("dockFiles",&json!(["/fixture/report.pdf"])));
        assert!(!valid("dockFiles",&json!(["/bad\tpath"])));
        assert!(valid("dockFolders",&json!([{"path":"/folder","sort":"name","descending":false,"presentation":"grid"}])));
        assert!(!valid("dockFolders",&json!([{"path":"relative"}])));
        assert!(valid_option("windows","tileGap",&json!(32)));
        assert!(!valid_option("windows","tileGap",&json!(33)));
        assert!(valid_option("menuBar","visibility",&json!("desktop")));
        assert!(!valid_option("menuBar","visibility",&json!("automatic")));
        assert!(valid_option("search","disabledProviders",&json!(["files","clipboard"])));
        assert!(!valid_option("search","disabledProviders",&json!(["files","files"])));
        assert_eq!(placement_commands(&json!({"tileMargins":true,"titlebarDoubleClick":"zoom"}),false),["lumen_window margins enable"]);
        assert_eq!(placement_commands(&json!({"tileMargins":true,"titlebarDoubleClick":"zoom"}),true).len(),2);
    }
    #[test]
    fn semantic_palette_keeps_text_readable() {
        let palettes: Value = serde_json::from_str(PALETTE).unwrap();
        for mode in ["light","dark"] {
            for (accent,_,_) in ACCENTS {
                for increased in [false,true] {
                    let colors = resolve_palette(&palettes,mode,&json!({"accent":accent,"increaseContrast":increased})).unwrap();
                    for foreground in ["text","muted","error","success","warning"] {
                        for surface in ["background","surface","field","hover"] {
                            assert!(contrast(colors[foreground].as_str().unwrap(),colors[surface].as_str().unwrap())>=4.5,
                                "{mode}/{accent}/{foreground}/{surface}");
                        }
                    }
                    assert!(contrast(colors["selectedText"].as_str().unwrap(),colors["accent"].as_str().unwrap())>=4.5);
                    assert!(contrast(colors["destructiveText"].as_str().unwrap(),colors["destructiveFill"].as_str().unwrap())>=4.5);
                    if increased {
                        for surface in ["background","surface","field"] {
                            assert!(contrast(colors["border"].as_str().unwrap(),colors[surface].as_str().unwrap())>=3.0);
                        }
                    }
                }
            }
        }
        let mut altered = palettes.clone();
        altered["light"]["selectedText"] = json!("#ffffff");
        assert_eq!(resolve_palette(&altered,"light",&json!({"accent":"red"})).unwrap()["selectedText"],"#ffffff");
        assert_eq!(resolve_palette(&altered,"light",&json!({"accent":"blue"})).unwrap()["selectedText"],"#000000");
    }
}
