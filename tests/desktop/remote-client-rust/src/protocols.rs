use crate::{
    Result,
    harness::{Harness, ensure, nodes, pause, s, save, wait},
};
use serde_json::{Value, json};
use std::{
    env,
    ffi::OsString,
    fs,
    net::TcpStream,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn args(values: &[String]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
const REMOTE_TEXT: &str =
    "remote café — Ελληνικά 漢字 العربية 🌍\ncombining\te\u{301}\n\nfinal line\n";
const LOCAL_TEXT: &str =
    "local naïve € → 日本語 עברית 🧭\ncombining\ta\u{308}\n\nlast local line\n";
pub fn target(f: &mut Harness) -> Result<()> {
    let local_display = f
        .env
        .get("WAYLAND_DISPLAY")
        .cloned()
        .ok_or("Local display")?;
    if f.o.protocol == "vnc" && f.o.viewer_workflow() {
        let runtime = PathBuf::from(f.env.get("XDG_RUNTIME_DIR").ok_or("Runtime")?);
        let existing = fs::read_dir(&runtime)?
            .flatten()
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>();
        let config = f.out.join("remote-sway.conf");
        fs::write(
            &config,
            "output HEADLESS-1 resolution 1280x720 position 0 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id=\"org.lumen.RemoteTarget\"] move to output HEADLESS-1, fullscreen enable\n",
        )?;
        let local_outputs = f.env.insert("WLR_HEADLESS_OUTPUTS".into(), "1".into());
        let launched = f.launch_s(
            f.o.tool("sway"),
            &["--unsupported-gpu", "-c", &s(&config)],
            "remote-sway.log",
        );
        if let Some(outputs) = local_outputs {
            f.env.insert("WLR_HEADLESS_OUTPUTS".into(), outputs);
        } else {
            f.env.remove("WLR_HEADLESS_OUTPUTS");
        }
        launched?;
        f.remote_ipc = Some(wait("independent remote IPC socket", || {
            Ok(fs::read_dir(&runtime)?
                .flatten()
                .find(|entry| {
                    !existing.contains(&entry.file_name())
                        && entry.file_name().to_string_lossy().starts_with("sway-ipc")
                })
                .map(|entry| entry.path()))
        })?);
        let remote = wait("independent remote Wayland socket", || {
            Ok(fs::read_dir(&runtime)?
                .flatten()
                .map(|entry| entry.file_name())
                .find(|name| {
                    !existing.contains(name)
                        && name.to_string_lossy().starts_with("wayland-")
                        && !name.to_string_lossy().ends_with(".lock")
                }))
        })?;
        let remote = remote.to_str().ok_or("Remote display name")?.to_owned();
        f.remote_display = Some(remote.clone());
        f.env.insert("WAYLAND_DISPLAY".into(), remote);
    }
    if f.o.protocol == "rdp" || f.o.scenario == "legacy-clipboard" {
        fs::create_dir("/bin")?;
        symlink(f.o.tool("posix-shell"), "/bin/sh")?;
        fs::create_dir_all("/share/X11/xkb/compiled")?;
        let xkb = f.out.join("xkb");
        for directory in ["rules", "compiled", "geometry"] {
            fs::create_dir_all(xkb.join(directory))?;
        }
        fs::write(
            xkb.join("geometry/fixture"),
            "default xkb_geometry \"fixture\" {description=\"Private US input fixture\";};\n",
        )?;
        let text = fs::read_to_string("/source/tests/desktop/fixtures/x11-us.xkb")?;
        let sections = [
            ("keycodes", "xkb_keycodes"),
            ("types", "xkb_types"),
            ("compat", "xkb_compatibility"),
            ("symbols", "xkb_symbols"),
        ];
        for (index, (directory, marker)) in sections.iter().enumerate() {
            let start = text.find(marker).ok_or("Keymap section")?;
            let end = if index + 1 < sections.len() {
                start
                    + text[start..]
                        .find(sections[index + 1].1)
                        .ok_or("Next keymap section")?
            } else {
                text.rfind("};").ok_or("Keymap end")?
            };
            fs::create_dir(xkb.join(directory))?;
            fs::write(
                xkb.join(directory).join("fixture"),
                format!("default {}\n", text[start..end].trim()),
            )?;
        }
        let mut rules = String::new();
        for (lhs, rhs) in [
            ("model", "keycodes"),
            ("model", "types"),
            ("model", "compat"),
            ("layout", "symbols"),
            ("model", "geometry"),
        ] {
            rules.push_str(&format!("! {lhs} = {rhs}\n  * = fixture\n"));
        }
        for name in ["base", "evdev"] {
            fs::write(xkb.join("rules").join(name), &rules)?;
        }
        f.launch_s(
            f.o.tool("xserver").join("bin/Xvfb"),
            &[
                ":55",
                "-screen",
                "0",
                "1280x720x24",
                "-xkbdir",
                &s(&xkb),
                "-nolisten",
                "unix",
                "-nolisten",
                "local",
                "-listen",
                "tcp",
                "-ac",
            ],
            "xserver.log",
        )?;
        wait("private loopback X server", || {
            Ok(
                TcpStream::connect_timeout(&"127.0.0.1:6055".parse()?, Duration::from_millis(100))
                    .ok()
                    .map(|_| ()),
            )
        })?;
        f.env.insert("DISPLAY".into(), "127.0.0.1:55".into());
        f.env.insert("GDK_BACKEND".into(), "x11".into());
    }
    let target = env::current_exe()?
        .parent()
        .ok_or("Fixture bin directory")?
        .join("lumen-remote-target");
    if ["text-clipboard", "legacy-clipboard"].contains(&f.o.scenario.as_str()) {
        let seed = f.private.join("remote-text.txt");
        fs::write(&seed, REMOTE_TEXT)?;
        f.launch_s(
            target,
            &["--multiline", "--seed-file", &s(&seed)],
            "target.log",
        )?;
    } else {
        f.launch_s(target, &[], "target.log")?;
    }
    if f.o.protocol == "rdp" || f.o.scenario == "legacy-clipboard" || f.remote_display.is_some() {
        wait("independent remote target", || Ok(f.target().ok()))?;
        f.env.insert("GDK_BACKEND".into(), "wayland".into());
        if f.remote_ipc.is_some() {
            let node = wait("independent remote window mapping", || {
                Ok(nodes(&f.remote_command(&["-r", "-t", "get_tree"])?)
                    .into_iter()
                    .find(|node| node["app_id"] == "org.lumen.RemoteTarget"))
            })?;
            let id = node["id"].as_u64().ok_or("Remote window identity")?;
            f.remote_command(&[&format!("[con_id={id}] focus")])?;
            let seats = f.remote_command(&["-r", "-t", "get_seats"])?;
            ensure(
                seats
                    .as_array()
                    .ok_or("Remote seats")?
                    .iter()
                    .any(|seat| seat["name"] == "seat0" && seat["focus"] == id),
                "Independent target lacks remote seat focus",
            )?;
            save(
                &f.out.join("remote-initial-tree.json"),
                &f.remote_command(&["-r", "-t", "get_tree"])?,
            )?;
            save(&f.out.join("remote-initial-seats.json"), &seats)?;
        }
    } else {
        wait("actual target window", || {
            Ok(nodes(&f.tree()?)
                .into_iter()
                .find(|node| node["app_id"] == "org.lumen.RemoteTarget"))
        })?;
    }
    f.env.insert("WAYLAND_DISPLAY".into(), local_display);
    Ok(())
}
pub fn run(f: &mut Harness) -> Result<()> {
    match f.o.protocol.as_str() {
        "vnc" => vnc(f),
        "rdp" => rdp(f),
        "ssh" => ssh(f),
        _ => Err("Unknown remote protocol".into()),
    }
}
fn negative_identity(f: &mut Harness, needle: &str) -> Result<Value> {
    let row = wait("explicit peer identity rejection", || {
        Ok(f.semantics()?
            .into_iter()
            .find(|row| row["name"].as_str().unwrap_or("").contains(needle)))
    })?;
    ensure(
        !f.log("remmina.log").contains("Connect signal emitted"),
        "Wrong peer authenticated",
    )?;
    ensure(
        !f.semantics()?
            .iter()
            .any(|row| row["role"] == "password text"),
        "Unverified peer requested credentials",
    )?;
    Ok(row)
}
fn vnc_start(f: &mut Harness, settings: &Path, control: &Path, log: &str) -> Result<usize> {
    let local_display = f
        .env
        .get("WAYLAND_DISPLAY")
        .cloned()
        .ok_or("Local display")?;
    let independent = f.remote_display.is_some();
    if let Some(display) = &f.remote_display {
        f.env.insert("WAYLAND_DISPLAY".into(), display.clone());
    }
    let result = f.launch_s(
        f.o.tool("wayvnc").join("bin/wayvnc"),
        &[
            "--log-level",
            "info",
            "--config",
            &s(settings),
            "--output",
            if independent {
                "HEADLESS-1"
            } else {
                "HEADLESS-2"
            },
            "--seat",
            if independent { "seat0" } else { "remote" },
            "--socket",
            &s(control),
            "127.0.0.1",
        ],
        log,
    );
    f.env.insert("WAYLAND_DISPLAY".into(), local_display);
    let index = result?;
    wait("actual encrypted VNC server", || {
        Ok(control.exists().then_some(()))
    })?;
    Ok(index)
}
fn vnc(f: &mut Harness) -> Result<()> {
    if f.o.scenario == "legacy-clipboard" {
        return legacy_clipboard(f);
    }
    let cert = f.private.join("server.crt");
    let key = f.private.join("server.key");
    let expected_ip = if f.o.scenario == "wrong-identity" {
        "127.0.0.2"
    } else {
        "127.0.0.1"
    };
    let ca = f.certificate(
        &cert,
        &key,
        "localhost",
        &format!("DNS:localhost,IP:{expected_ip}"),
    )?;
    let settings = f.private.join("wayvnc.conf");
    fs::write(
        &settings,
        format!(
            "address=127.0.0.1\nport=5905\nenable_auth=true\nusername=fixture\npassword={}\nprivate_key_file={}\ncertificate_file={}\n",
            f.password,
            key.display(),
            cert.display()
        ),
    )?;
    fs::set_permissions(&settings, fs::Permissions::from_mode(0o600))?;
    let control = PathBuf::from("/run/fixture/remote-vnc");
    let server = vnc_start(f, &settings, &control, "wayvnc.log")?;
    fs::write(
        &f.profile,
        format!(
            "[remmina]\nname=Disposable VNC\nprotocol=VNC\nserver=127.0.0.1:5905\nusername=fixture\ncacert={}\ndisablepasswordstoring=1\ncolordepth=32\nquality=9\nscale=1\nviewmode=1\ndisableclipboard=0\n",
            ca.display()
        ),
    )?;
    f.connect()?;
    if f.o.scenario == "wrong-identity" {
        let rejection = negative_identity(f, "Could not authenticate")?;
        ensure(
            !f.log("wayvnc.log").contains("authenticated"),
            "Wrong IP SAN authenticated",
        )?;
        return save(
            &f.out.join("acceptance.json"),
            &json!({"protocol":"VNC","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"WayVNC","trustedCA":true,"wrongIPSAN":true,"credentialPrompt":false,"rejection":rejection}),
        );
    }
    let password = if f.o.scenario != "wrong-password" {
        f.password.clone()
    } else {
        "Deliberately-wrong-fixture-password".into()
    };
    f.password(&password)?;
    if f.o.scenario == "wrong-password" {
        wait("real server rejects password", || {
            Ok(f.log("wayvnc.log")
                .contains("Invalid username or password")
                .then_some(()))
        })?;
        ensure(
            !f.log("wayvnc.log").contains("authenticated"),
            "Wrong credential authenticated",
        )?;
        return save(
            &f.out.join("acceptance.json"),
            &json!({"protocol":"VNC","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"WayVNC","authenticated":false}),
        );
    }
    wait("real VNC server authentication", || {
        Ok(f.log("wayvnc.log")
            .contains("User \"fixture\" authenticated")
            .then_some(()))
    })?;
    let original = f.safe_target()?;
    if f.o.viewer_workflow() {
        return viewer_workflow(f);
    }
    pause(400);
    f.capture("authenticated.png")?;
    let pixels = f.pixels("authenticated.png")?;
    f.stop(server)?;
    wait("actual VNC transport loss", || {
        Ok((f
            .log("remmina.log")
            .contains("VNC server closed connection")
            || f.log("remmina.log").to_lowercase().contains("disconnected"))
        .then_some(()))
    })?;
    let semantics = f.semantics()?;
    save(
        &f.out.join("network-loss-semantics.json"),
        &json!(semantics),
    )?;
    f.client_stop()?;
    let _ = fs::remove_file(&control);
    vnc_start(f, &settings, &control, "wayvnc-restarted.log")?;
    f.connect()?;
    reconnect_password(f, "wayvnc-restarted.log", "User \"fixture\" authenticated")?;
    let final_target = f.safe_target()?;
    ensure(
        original["pid"] == final_target["pid"] && original["text"] == final_target["text"],
        "Remote target changed during reconnect",
    )?;
    pause(400);
    f.capture("reconnected.png")?;
    f.pixels("reconnected.png")?;
    save(
        &f.out.join("acceptance.json"),
        &json!({"protocol":"VNC","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"WayVNC","authenticated":true,"numericIPVerified":true,"trustedCA":true,"palettePixels":pixels,"transportLossObserved":true,"reconnect":"manual same-profile client restart","serverCertificateUnchanged":true,"remoteTargetIdentityRetained":true}),
    )
}
fn reconnect_password(f: &mut Harness, log: &str, success: &str) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end && !f.log(log).contains(success) {
        if f.semantics()?
            .iter()
            .any(|row| row["role"] == "password text")
        {
            let password = f.password.clone();
            f.password(&password)?;
        }
        pause(150);
    }
    ensure(
        f.log(log).contains(success),
        "Manual reconnect did not authenticate",
    )
}
fn rdp_start(f: &mut Harness, sam: &Path, log: &str) -> Result<usize> {
    let index = f.launch(
        f.o.tool("rdp").join("bin/freerdp-shadow-cli"),
        &args(&[
            "/port:3395".into(),
            "/bind-address:127.0.0.1".into(),
            format!("/sam-file:{}", sam.display()),
            "/sec:nla".into(),
        ]),
        log,
    )?;
    pause(500);
    ensure(
        f.children[index].try_wait()?.is_none(),
        "Actual RDP server failed",
    )?;
    Ok(index)
}
fn rdp(f: &mut Harness) -> Result<()> {
    if f.o.viewer_workflow() {
        return rdp_clipboard(f);
    }
    let sam = f.call(
        f.o.tool("rdp").join("bin/winpr-hash"),
        &["-u", "fixture", "-p", &f.password, "-f", "sam"],
    )?;
    f.guard.add(sam.clone())?;
    let sam_file = f.private.join("SAM");
    fs::write(&sam_file, format!("{sam}\n"))?;
    fs::set_permissions(&sam_file, fs::Permissions::from_mode(0o600))?;
    let cert_dir =
        PathBuf::from(f.env.get("XDG_CONFIG_HOME").ok_or("Private config")?).join("freerdp/shadow");
    fs::create_dir_all(&cert_dir)?;
    let wrong = f.o.scenario == "wrong-identity";
    let ca = f.certificate(
        &cert_dir.join("shadow.crt"),
        &cert_dir.join("shadow.key"),
        if wrong { "other.invalid" } else { "localhost" },
        if wrong {
            "DNS:other.invalid"
        } else {
            "DNS:localhost,IP:127.0.0.1"
        },
    )?;
    let trust = cert_dir
        .parent()
        .ok_or("Certificate directory")?
        .join("certs");
    fs::create_dir(&trust)?;
    fs::copy(ca, trust.join("fixture-ca.pem"))?;
    f.quiet(f.o.tool("tls").join("bin/openssl"), &["rehash", &s(&trust)])?;
    let server = rdp_start(f, &sam_file, "rdp-server.log")?;
    fs::write(
        &f.profile,
        "[remmina]\nname=Disposable RDP\nprotocol=RDP\nserver=localhost:3395\nusername=fixture\nsecurity=nla\ncolordepth=32\nsound=off\nmicrophone=0\nresolution_mode=0\nresolution_width=1280\nresolution_height=720\nscale=1\nviewmode=1\ncert_ignore=0\n",
    )?;
    f.connect()?;
    if wrong {
        let warning = negative_identity(f, "Accept certificate?")?;
        return save(
            &f.out.join("acceptance.json"),
            &json!({"protocol":"RDP","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"FreeRDP shadow","authenticated":false,"trustedCA":true,"wrongDNSIdentity":true,"certificateApprovalRequired":true,"warning":warning}),
        );
    }
    let row = wait("protected NLA credential entry", || {
        Ok(f.semantics()?
            .into_iter()
            .find(|row| row["role"] == "password text"))
    })?;
    ensure(
        !f.semantics()?.iter().any(|row| {
            row["name"]
                .as_str()
                .unwrap_or("")
                .contains("Accept certificate?")
        }),
        "Valid trusted peer requested certificate override",
    )?;
    f.focus(&row)?;
    let password = if f.o.scenario != "wrong-password" {
        f.password.clone()
    } else {
        "Deliberately-wrong-fixture-password".into()
    };
    f.password(&password)?;
    if f.o.scenario == "wrong-password" {
        wait("actual NLA bad credential rejection", || {
            Ok((f.log("rdp-server.log").contains("STATUS_LOGON_FAILURE")
                || f.log("remmina.log").contains("ERRCONNECT_LOGON_FAILURE"))
            .then_some(()))
        })?;
        ensure(
            !f.log("remmina.log").contains("Connect signal emitted"),
            "Wrong NLA password authenticated",
        )?;
        return save(
            &f.out.join("acceptance.json"),
            &json!({"protocol":"RDP","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"FreeRDP shadow","authenticated":false,"nlaWrongPasswordRejected":true}),
        );
    }
    wait("actual NLA connection", || {
        Ok(f.log("remmina.log")
            .contains("Connect signal emitted")
            .then_some(()))
    })?;
    let original = f.safe_target()?;
    if f.o.viewer_workflow() {
        return viewer_workflow(f);
    }
    pause(400);
    f.capture("authenticated.png")?;
    let pixels = f.pixels("authenticated.png")?;
    let offset = f.log("remmina.log").len();
    f.stop(server)?;
    wait("actual RDP transport loss", || {
        let log = f.log("remmina.log");
        Ok([
            "BIO_read",
            "transport_read_layer",
            "ERRCONNECT",
            "disconnected",
            "Connection reset",
        ]
        .iter()
        .any(|marker| log.get(offset..).unwrap_or("").contains(marker))
        .then_some(()))
    })?;
    let state = f.semantics()?;
    save(&f.out.join("network-loss-semantics.json"), &json!(state))?;
    f.client_stop()?;
    rdp_start(f, &sam_file, "rdp-server-restarted.log")?;
    f.connect()?;
    let client_log = f.logs.last().ok_or("Client log")?.clone();
    reconnect_password(f, &client_log, "Connect signal emitted")?;
    ensure(
        !f.semantics()?.iter().any(|row| {
            row["name"]
                .as_str()
                .unwrap_or("")
                .contains("Accept certificate?")
        }),
        "Reconnect requested certificate bypass",
    )?;
    let final_target = f.safe_target()?;
    ensure(
        original["pid"] == final_target["pid"] && original["text"] == final_target["text"],
        "Remote target identity changed",
    )?;
    pause(400);
    f.capture("reconnected.png")?;
    f.pixels("reconnected.png")?;
    save(
        &f.out.join("acceptance.json"),
        &json!({"protocol":"RDP","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"FreeRDP shadow","windowsOS":false,"authenticated":true,"security":"NLA","trustedCA":true,"certificateOverride":false,"nondefaultPort":3395,"palettePixels":pixels,"transportLossObserved":true,"reconnect":"manual same-profile client restart","serverCertificateUnchanged":true,"remoteTargetIdentityRetained":true}),
    )
}
fn legacy_clipboard(f: &mut Harness) -> Result<()> {
    let password_file = f.private.join("legacy-vnc-password");
    f.quiet(
        f.o.tool("x11vnc").join("bin/x11vnc"),
        &["-storepasswd", &f.password, &s(&password_file)],
    )?;
    // The disposable Xvfb has no login/display manager whose windows need protection.
    f.env.insert("X11VNC_AVOID_WINDOWS".into(), "never".into());
    let legacy_server = f.launch_s(
        f.o.tool("x11vnc").join("bin/x11vnc"),
        &[
            "-display",
            "127.0.0.1:55",
            "-rfbport",
            "5906",
            "-listen",
            "127.0.0.1",
            "-rfbauth",
            &s(&password_file),
            "-forever",
            "-shared",
            "-noxdamage",
            "-noshm",
            "-debug_sel",
        ],
        "legacy-x11vnc.log",
    )?;
    f.env.remove("X11VNC_AVOID_WINDOWS");
    wait("private legacy VNC server", || {
        Ok(
            TcpStream::connect_timeout(&"127.0.0.1:5906".parse()?, Duration::from_millis(100))
                .ok()
                .map(|_| ()),
        )
    })?;
    fs::write(
        &f.profile,
        "[remmina]\nname=Disposable legacy clipboard\nprotocol=VNC\nserver=127.0.0.1:5906\ncolordepth=32\nquality=9\nscale=1\nviewmode=1\ndisableclipboard=0\ndisablepasswordstoring=1\n",
    )?;
    f.connect()?;
    let password = f.password.clone();
    f.password(&password)?;
    wait("actual authenticated legacy viewer", || {
        Ok(f.log("remmina.log")
            .contains("Connect signal emitted")
            .then_some(()))
    })?;
    wait("actual legacy X11 selection owner readiness", || {
        Ok(f.log("legacy-x11vnc.log")
            .contains("created selwin:")
            .then_some(()))
    })?;
    let viewer = focus_app(f, "org.remmina.Remmina")?;
    let original = f.safe_target()?;
    let target = env::current_exe()?
        .parent()
        .ok_or("Fixture bin")?
        .join("lumen-remote-target");
    const LEGACY: &str = "legacy café £\ncompatible\ttext\n";
    let seed = f.private.join("legacy-source.txt");
    fs::write(&seed, LEGACY)?;
    let local = f.launch_s(
        &target,
        &["--local", "--multiline", "--seed-file", &s(&seed)],
        "legacy-local.log",
    )?;
    focus_app(f, "org.lumen.LocalTarget")?;
    chord(f, "a")?;
    chord(f, "c")?;
    pause(1500);
    focus_viewer(f)?;
    let compatible = paste_until(f, false, LEGACY)?;
    let before_reject = f.safe_target()?;
    save(
        &f.out.join("legacy-compatible-receipt.json"),
        &before_reject,
    )?;
    let raw_latin1 = b"Set CLIPBOARD 'legacy caf\xe9 \xa3\ncompatible\ttext\n'";
    let wire_received = fs::read(f.out.join("legacy-x11vnc.log"))?
        .windows(raw_latin1.len())
        .any(|bytes| bytes == raw_latin1);
    ensure(
        wire_received,
        "Actual legacy server did not receive exact Latin-1 bytes",
    )?;
    ensure(compatible, "Representable legacy clipboard text differs")?;
    f.stop(local)?;
    wait("legacy local document bus release", || {
        Ok(f.target_named("org.lumen.LocalTarget")
            .is_err()
            .then_some(()))
    })?;
    fs::write(&seed, LOCAL_TEXT)?;
    let unsupported_local = f.launch_s(
        &target,
        &["--local", "--multiline", "--seed-file", &s(&seed)],
        "legacy-unsupported-local.log",
    )?;
    focus_app(f, "org.lumen.LocalTarget")?;
    wait("unsupported local source ready", || {
        Ok((f.target_named("org.lumen.LocalTarget")?["text"] == LOCAL_TEXT).then_some(()))
    })?;
    chord(f, "a")?;
    pause(1500);
    chord(f, "c")?;
    // Wayland grants the clipboard selection to the focused receiving client.
    focus_viewer(f)?;
    let warning = wait("explicit unsupported legacy text warning", || {
        save(
            &f.out.join("legacy-unsupported-local-receipt.json"),
            &f.target_named("org.lumen.LocalTarget")?,
        )?;
        save(&f.out.join("legacy-warning-tree.json"), &f.tree()?)?;
        let semantics = f.semantics()?;
        save(
            &f.out.join("legacy-warning-semantics.json"),
            &json!(semantics),
        )?;
        Ok(semantics.into_iter().find(|row| {
            row["name"]
                .as_str()
                .unwrap_or("")
                .contains("only supports Latin-1")
        }))
    })?;
    let dialog = wait("owned modal clipboard warning window", || {
        Ok(nodes(&f.tree()?)
            .into_iter()
            .find(|node| node["name"] == "Clipboard transfer"))
    })?;
    save(&f.out.join("legacy-warning-tree.json"), &f.tree()?)?;
    ensure(
        dialog["pid"] == viewer["pid"]
            && dialog["lumen_relationships"]["parent"] == viewer["id"]
            && (dialog["lumen_relationships"]["modal"].is_null()
                || dialog["lumen_relationships"]["modal"] == true),
        "Clipboard warning is not modal to the originating connection",
    )?;
    let modal_semantics = f.semantics()?;
    ensure(
        modal_semantics
            .iter()
            .any(|row| row["states"]["modal"] == true && row["pid"] == viewer["pid"]),
        "Actual GTK warning does not advertise modal accessibility state",
    )?;
    ensure(
        nodes(&f.tree()?)
            .iter()
            .filter(|node| node["name"] == "Clipboard transfer")
            .count()
            == 1,
        "Clipboard rejection created duplicate warning windows",
    )?;
    save(&f.out.join("legacy-warning-tree.json"), &f.tree()?)?;
    let unchanged = f.safe_target()?;
    ensure(
        unchanged["text"] == before_reject["text"] && unchanged["pid"] == original["pid"],
        "Unsupported clipboard changed the remote document",
    )?;
    let close = wait("legacy warning close button", || {
        Ok(f.semantics()?
            .into_iter()
            .find(|row| row["role"] == "button" && row["name"] == "Close"))
    })?;
    let dialog_id = dialog["id"].as_u64().ok_or("Captured warning identity")?;
    f.ipc(&[&format!("[con_id={dialog_id}] focus")])?;
    let reference = &close["reference"];
    ensure(
        f.observer.as_ref().ok_or("Observer")?.focus(
            &(
                reference[0].as_str().ok_or("Close owner")?.to_owned(),
                reference[1].as_str().ok_or("Close path")?.to_owned(),
            ),
            Duration::from_millis(500),
        )?,
        "Captured warning Close control refused focus",
    )?;
    f.keyboard
        .as_mut()
        .ok_or("Keyboard")?
        .key(&["-k", "Return"])?;
    focus_viewer(f)?;
    let rejected_paste_preserved = paste_until(f, false, LEGACY)?;
    ensure(
        rejected_paste_preserved,
        "Rejected Unicode text replaced the legacy server clipboard",
    )?;
    chord(f, "a")?;
    f.keyboard
        .as_mut()
        .ok_or("Keyboard")?
        .type_text("still-connected")?;
    wait("legacy connection input after clipboard rejection", || {
        Ok((f.safe_target()?["text"] == "still-connected").then_some(()))
    })?;
    let input_receipt = f.safe_target()?;
    focus_app(f, "org.lumen.LocalTarget")?;
    chord(f, "a")?;
    f.keyboard
        .as_mut()
        .ok_or("Keyboard")?
        .type_text("new-compatible-selection")?;
    chord(f, "a")?;
    chord(f, "c")?;
    focus_viewer(f)?;
    ensure(
        paste_until(f, false, "new-compatible-selection")?,
        "Changed compatible clipboard remained blocked after a rejection",
    )?;
    let changed_receipt = f.safe_target()?;
    f.stop(unsupported_local)?;
    wait("changed document bus release", || {
        Ok(f.target_named("org.lumen.LocalTarget")
            .is_err()
            .then_some(()))
    })?;
    f.launch_s(
        &target,
        &["--local", "--multiline", "--seed-file", &s(&seed)],
        "legacy-disconnect-local.log",
    )?;
    focus_app(f, "org.lumen.LocalTarget")?;
    chord(f, "a")?;
    chord(f, "c")?;
    focus_viewer(f)?;
    let disconnect_dialog = wait("warning before actual server loss", || {
        Ok(nodes(&f.tree()?)
            .into_iter()
            .find(|node| node["name"] == "Clipboard transfer"))
    })?;
    save(
        &f.out.join("legacy-before-disconnect-tree.json"),
        &f.tree()?,
    )?;
    f.stop(legacy_server)?;
    wait("clipboard warning removed on connection loss", || {
        Ok((!nodes(&f.tree()?)
            .iter()
            .any(|node| node["name"] == "Clipboard transfer"))
        .then_some(()))
    })?;
    save(&f.out.join("legacy-after-disconnect-tree.json"), &f.tree()?)?;
    let after_disconnect = f.safe_target()?;
    ensure(
        after_disconnect["pid"] == original["pid"]
            && after_disconnect["text"] == changed_receipt["text"],
        "Disconnect warning lifecycle changed the remote document",
    )?;
    save(
        &f.out.join("legacy-clipboard-acceptance.json"),
        &json!({
        "protocol":"VNC", "actualServer":"x11vnc with X11 TARGETS property type repair; legacy RFB capabilities unchanged", "transport":"unencrypted RFB inside disposable loopback namespace only",
        "independentDisplays":true, "representableLatin1Source":LEGACY, "representableReceipt":before_reject,
        "actualServerReceivedLatin1Hex":"6c656761637920636166e920a30a636f6d70617469626c6509746578740a",
        "actualServerReceivedLatin1Bytes":wire_received,
        "unrepresentableSource":LOCAL_TEXT, "rejection":warning, "unchangedReceipt":unchanged,
        "originatingViewer":viewer, "ownedModalDialog":dialog,
        "connectionInputReceipt":input_receipt, "changedCompatibleClipboardReceipt":changed_receipt, "remoteTargetIdentityRetained":true,
        "warningBeforeActualServerLoss":disconnect_dialog, "warningRemovedOnDisconnect":true,
        "afterDisconnectTarget":after_disconnect,
        "rejectedPastePreservedPriorClipboard":rejected_paste_preserved,
        "notClaimed":["encrypted legacy connection", "HTML", "files", "Windows"]}),
    )
}
fn rdp_clipboard(f: &mut Harness) -> Result<()> {
    let config = PathBuf::from("/run/fixture/xrdp");
    fs::create_dir_all(&config)?;
    let upstream_config = f.o.tool("xrdp").join("etc/xrdp");
    for entry in fs::read_dir(upstream_config)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let copy = config.join(entry.file_name());
            fs::copy(entry.path(), &copy)?;
            fs::set_permissions(copy, fs::Permissions::from_mode(0o600))?;
        }
    }
    let sesman = config.join("sesman.ini");
    fs::write(
        &sesman,
        fs::read_to_string(&sesman)?.replace("#EnableFuseMount=false", "EnableFuseMount=false"),
    )?;
    let uid = unsafe { libc::getuid() };
    fs::create_dir_all(format!("/var/run/xrdp/{uid}"))?;
    fs::create_dir_all(format!("/run/xrdp/{uid}"))?;
    let cert = f.private.join("xrdp.crt");
    let key = f.private.join("xrdp.key");
    let ca = f.certificate(&cert, &key, "localhost", "DNS:localhost,IP:127.0.0.1")?;
    let trust =
        PathBuf::from(f.env.get("XDG_CONFIG_HOME").ok_or("Private config")?).join("freerdp/certs");
    fs::create_dir_all(&trust)?;
    fs::copy(ca, trust.join("fixture-ca.pem"))?;
    f.quiet(f.o.tool("tls").join("bin/openssl"), &["rehash", &s(&trust)])?;
    let vnc_password = f.private.join("x11vnc-password");
    f.quiet(
        f.o.tool("x11vnc").join("bin/x11vnc"),
        &["-storepasswd", &f.password, &s(&vnc_password)],
    )?;
    f.launch_s(
        f.o.tool("x11vnc").join("bin/x11vnc"),
        &[
            "-display",
            "127.0.0.1:55",
            "-rfbport",
            "5906",
            "-listen",
            "127.0.0.1",
            "-rfbauth",
            &s(&vnc_password),
            "-forever",
            "-shared",
            "-noxdamage",
            "-noshm",
        ],
        "x11vnc.log",
    )?;
    wait("private authenticated X11 VNC backend", || {
        Ok(
            TcpStream::connect_timeout(&"127.0.0.1:5906".parse()?, Duration::from_millis(100))
                .ok()
                .map(|_| ()),
        )
    })?;
    f.env.insert("CHANSRV_LOG_PATH".into(), s(&f.out));
    f.launch_s(
        f.o.tool("xrdp").join("bin/xrdp-chansrv"),
        &[],
        "xrdp-chansrv.log",
    )?;
    let socket = wait("actual clipboard channel server", || {
        Ok([
            format!("/var/run/xrdp/{uid}/xrdp_chansrv_socket_55"),
            format!("/run/xrdp/{uid}/xrdp_chansrv_socket_55"),
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.exists()))
    })?;
    let rsa = config.join("rsakeys.ini");
    f.quiet(
        f.o.tool("xrdp").join("bin/xrdp-keygen"),
        &["xrdp", &s(&rsa)],
    )?;
    let ini = config.join("xrdp.ini");
    fs::write(
        &ini,
        format!(
            "[Globals]\nini_version=1\nfork=true\nport=tcp://127.0.0.1:3395\nautorun=fixture\nsecurity_layer=tls\ncrypt_level=high\ncertificate={}\nkey_file={}\nrsakeys_ini={}\nallow_channels=true\nmax_bpp=32\n[Logging]\nLogFile={}\nLogLevel=DEBUG\nEnableSyslog=false\n[Channels]\ncliprdr=true\nrdpdr=false\nrdpsnd=false\ndrdynvc=false\nrail=false\n[fixture]\nname=Disposable clipboard session\nlib={}\nip=127.0.0.1\nport=5906\nusername=fixture\npassword=ask\nchansrvport={}\n",
            cert.display(),
            key.display(),
            rsa.display(),
            f.out.join("xrdp-provider.log").display(),
            "libvnc.so",
            socket.display()
        ),
    )?;
    f.launch_s(
        f.o.tool("xrdp").join("bin/xrdp"),
        &["--nodaemon", "--config", &s(&ini)],
        "xrdp.log",
    )?;
    wait("actual RDP clipboard server", || {
        Ok(
            TcpStream::connect_timeout(&"127.0.0.1:3395".parse()?, Duration::from_millis(100))
                .ok()
                .map(|_| ()),
        )
    })?;
    fs::write(
        &f.profile,
        "[remmina]\nname=Disposable RDP clipboard\nprotocol=RDP\nserver=localhost:3395\nusername=fixture\nsecurity=tls\ncolordepth=32\nsound=off\nmicrophone=0\ndisableclipboard=0\nresolution_mode=0\nresolution_width=1280\nresolution_height=720\nviewmode=1\ncert_ignore=0\ndisablepasswordstoring=1\n",
    )?;
    f.connect()?;
    let password = f.password.clone();
    f.password(&password)?;
    wait("real clipboard-capable RDP session", || {
        Ok(f.log("remmina.log")
            .contains("Connect signal emitted")
            .then_some(()))
    })?;
    wait("actual authenticated RDP backend", || {
        Ok(f.log("xrdp-provider.log")
            .contains("VNC connection complete, connected ok")
            .then_some(()))
    })?;
    viewer_workflow(f)
}
fn viewer_workflow(f: &mut Harness) -> Result<()> {
    if f.o.scenario == "text-clipboard" {
        text_clipboard(f)
    } else {
        interactions(f)
    }
}
fn text_clipboard(f: &mut Harness) -> Result<()> {
    let original = f.safe_target()?;
    ensure(
        original["text"] == REMOTE_TEXT,
        "Seeded remote document differs",
    )?;
    focus_viewer(f)?;
    chord(f, "a")?;
    chord(f, "c")?;
    pause(1500);
    let target = env::current_exe()?
        .parent()
        .ok_or("Fixture bin")?
        .join("lumen-remote-target");
    let local = f.launch_s(&target, &["--local", "--multiline"], "local-corpus.log")?;
    focus_app(f, "org.lumen.LocalTarget")?;
    let remote_to_local = paste_until(f, true, REMOTE_TEXT)?;
    let received_local = f.target_named("org.lumen.LocalTarget")?;
    f.stop(local)?;
    wait("local document bus release", || {
        Ok(f.target_named("org.lumen.LocalTarget")
            .is_err()
            .then_some(()))
    })?;
    let seed = f.private.join("local-text.txt");
    fs::write(&seed, LOCAL_TEXT)?;
    let reverse_local = f.launch_s(
        target,
        &["--local", "--multiline", "--seed-file", &s(&seed)],
        "local-corpus-reverse.log",
    )?;
    focus_app(f, "org.lumen.LocalTarget")?;
    wait("seeded local document", || {
        Ok((f.target_named("org.lumen.LocalTarget")?["text"] == LOCAL_TEXT).then_some(()))
    })?;
    chord(f, "a")?;
    chord(f, "c")?;
    pause(1500);
    focus_viewer(f)?;
    let local_to_remote = paste_until(f, false, LOCAL_TEXT)?;
    let received_remote = f.safe_target()?;
    ensure(
        original["pid"] == received_remote["pid"],
        "Remote document restarted during text transfer",
    )?;
    let mut repeated_remote = Value::Null;
    if remote_to_local && local_to_remote && f.o.protocol == "vnc" {
        f.stop(reverse_local)?;
        wait("reverse document bus release", || {
            Ok(f.target_named("org.lumen.LocalTarget")
                .is_err()
                .then_some(()))
        })?;
        focus_viewer(f)?;
        chord(f, "a")?;
        chord(f, "c")?;
        let target = env::current_exe()?
            .parent()
            .ok_or("Fixture bin")?
            .join("lumen-remote-target");
        f.launch_s(
            target,
            &["--local", "--multiline"],
            "local-corpus-repeat.log",
        )?;
        focus_app(f, "org.lumen.LocalTarget")?;
        let passed = paste_until(f, true, LOCAL_TEXT)?;
        repeated_remote = f.target_named("org.lumen.LocalTarget")?;
        ensure(
            passed,
            "Later remote copy was suppressed after changed local selection",
        )?;
    }
    save(
        &f.out.join("text-clipboard-acceptance.json"),
        &json!({"protocol":f.o.protocol,"scenario":f.o.scenario,"remoteToLocal":remote_to_local,"localToRemote":local_to_remote,"remoteText":REMOTE_TEXT,"localText":LOCAL_TEXT,"localReceipt":received_local,"remoteReceipt":received_remote,"laterRemoteCopyAfterChangedLocalSelection":repeated_remote,"remoteTargetIdentityRetained":true,"sourceBytesExactRemoteToLocal":received_local["text"]==REMOTE_TEXT,"sourceBytesExactLocalToRemote":received_remote["text"]==LOCAL_TEXT,"allowedLineEndingConversion":if f.o.protocol=="rdp" {"local-to-remote LF to CRLF only"} else {"none"},"independentDisplays":true,"corpus":["Latin diacritics","Greek","CJK","RTL scripts","supplementary-plane emoji","combining marks","multiline","tabs","blank lines","trailing newline"],"notClaimed":["HTML","files","Windows"]}),
    )?;
    ensure(
        remote_to_local && local_to_remote,
        "Unicode/multiline clipboard differs; exact receipts retained",
    )
}
fn focus_app(f: &Harness, app: &str) -> Result<Value> {
    let node = wait("interaction window mapping", || {
        Ok(nodes(&f.tree()?)
            .into_iter()
            .find(|node| node["app_id"] == app))
    })?;
    let id = node["id"].as_u64().ok_or("Interaction window identity")?;
    f.ipc(&[&format!("[con_id={id}] focus")])?;
    Ok(node)
}
fn chord(f: &mut Harness, key: &str) -> Result<()> {
    let keyboard = f.keyboard.as_mut().ok_or("Keyboard")?;
    // Remote protocols forward modifier keyvals; a local GDK mask alone is insufficient.
    keyboard.send("k", 29, 1)?;
    keyboard.key(&["-M", "ctrl", "-k", key, "-m", "ctrl"])?;
    keyboard.send("k", 29, 0)?;
    Ok(())
}
fn focus_viewer(f: &mut Harness) -> Result<()> {
    focus_app(f, "org.remmina.Remmina")?;
    let row = wait("remote drawing surface", || {
        Ok(f.semantics()?
            .into_iter()
            .find(|row| row["role"] == "drawing area" && row["states"]["focusable"] == true))
    })?;
    f.focus(&row)
}
fn fullscreen_key(f: &mut Harness) -> Result<()> {
    let keyboard = f.keyboard.as_mut().ok_or("Keyboard")?;
    keyboard.send("k", 88, 1)?;
    keyboard.key(&["-k", "f"])?;
    keyboard.send("k", 88, 0)?;
    Ok(())
}
fn clipboard_text_matches(actual: &str, expected: &str, rdp_newlines: bool) -> bool {
    // CF_UNICODETEXT uses CR/LF. Permit only this whole-string transformation;
    // raw receipts, combining sequences, whitespace and all codepoints stay intact.
    actual == expected
        || (rdp_newlines && !expected.contains('\r') && actual == expected.replace('\n', "\r\n"))
}
fn paste_until(f: &mut Harness, local: bool, expected: &str) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        chord(f, "a")?;
        chord(f, "v")?;
        pause(100);
        let state = if local {
            f.target_named("org.lumen.LocalTarget")?
        } else {
            f.safe_target()?
        };
        if clipboard_text_matches(
            state["text"].as_str().ok_or("Document text")?,
            expected,
            !local && f.o.protocol == "rdp" && f.o.scenario == "text-clipboard",
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod text_contract_tests {
    use super::clipboard_text_matches;
    #[test]
    fn only_explicit_rdp_whole_string_crlf_conversion_is_permitted() {
        let expected = "café\te\u{301} 漢 🌍\n\nend\n";
        let wire = expected.replace('\n', "\r\n");
        assert!(clipboard_text_matches(expected, expected, false));
        assert!(clipboard_text_matches(&wire, expected, true));
        assert!(!clipboard_text_matches(&wire, expected, false));
        for damaged in [
            expected.trim_end().to_owned(),
            expected.replace('\t', " "),
            expected.replace("e\u{301}", "é"),
            expected.replacen('\n', "\r\n", 1),
            expected.replace("\n\n", "\n"),
            expected.replace('🌍', "?"),
        ] {
            assert!(!clipboard_text_matches(&damaged, expected, true));
        }
    }
}
fn interactions(f: &mut Harness) -> Result<()> {
    let original_target = f.safe_target()?;
    let before = focus_app(f, "org.remmina.Remmina")?;
    focus_viewer(f)?;
    pause(400);
    f.capture("interaction-start.png")?;
    save(
        &f.out.join("interaction-semantics.json"),
        &json!(f.semantics()?),
    )?;
    f.keyboard
        .as_mut()
        .ok_or("Keyboard")?
        .type_text("remote-input-receipt")?;
    wait("actual protocol keyboard routing", || {
        Ok((f.target()?["text"] == "remote-input-receipt").then_some(()))
    })?;
    f.keyboard.as_mut().ok_or("Keyboard")?.key(&["-k", "F11"])?;
    wait("actual protocol function key", || {
        Ok(f.target()?["keys"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key["keyval"] == 65480))
            .then_some(()))
    })?;
    fullscreen_key(f)?;
    let full = wait("viewer fullscreen entry", || {
        Ok(nodes(&f.tree()?)
            .into_iter()
            .find(|node| node["app_id"] == "org.remmina.Remmina" && node["fullscreen_mode"] == 1))
    })?;
    f.capture("viewer-fullscreen.png")?;
    fullscreen_key(f)?;
    let exited = wait("viewer fullscreen exit", || {
        Ok(nodes(&f.tree()?)
            .into_iter()
            .find(|node| node["app_id"] == "org.remmina.Remmina" && node["fullscreen_mode"] == 0))
    })?;
    focus_viewer(f)?;
    chord(f, "a")?;
    chord(f, "c")?;
    pause(1500);
    let target = env::current_exe()?
        .parent()
        .ok_or("Fixture bin")?
        .join("lumen-remote-target");
    f.launch_s(target, &["--local"], "local-clipboard-target.log")?;
    wait("independent local clipboard target", || {
        Ok(f.target_named("org.lumen.LocalTarget").ok())
    })?;
    focus_app(f, "org.lumen.LocalTarget")?;
    let remote_to_local = paste_until(f, true, "remote-input-receipt")?;
    let received_local = f.target_named("org.lumen.LocalTarget")?;
    chord(f, "a")?;
    f.keyboard
        .as_mut()
        .ok_or("Keyboard")?
        .type_text("local-clipboard-receipt")?;
    chord(f, "a")?;
    chord(f, "c")?;
    pause(1500);
    focus_viewer(f)?;
    let local_to_remote = paste_until(f, false, "local-clipboard-receipt")?;
    let received_remote = f.safe_target()?;
    ensure(
        original_target["pid"] == received_remote["pid"],
        "Remote target restarted during viewer interactions",
    )?;
    save(
        &f.out.join("interaction-acceptance.json"),
        &json!({"protocol":f.o.protocol,"actualProvider":"Remmina","actualServer":if f.o.protocol=="vnc" {"WayVNC"} else {"xrdp with x11vnc and xrdp-chansrv"},"clipboardDisplaysIndependent":f.o.protocol=="rdp" || f.remote_display.is_some(),"nativeCompositors":if f.remote_display.is_some() {2} else {1},"remoteTargetIdentityRetained":original_target["pid"]==received_remote["pid"],"windowsOS":false,"physicalInput":true,"functionKeyF11":true,"fullscreenEntryAndExit":true,"fullscreenShortcut":"private configured F12+f host chord","originalViewerID":before["id"],"fullscreenViewerID":full["id"],"exitedViewerID":exited["id"],"remoteToLocalClipboard":remote_to_local,"localToRemoteClipboard":local_to_remote,"localReceipt":received_local,"remoteReceipt":received_remote,"hostClipboardAccess":false,"globalHostKeyClaim":false}),
    )?;
    ensure(
        remote_to_local && local_to_remote,
        "Actual clipboard direction failed; retained interaction diagnostics",
    )
}
fn sshkey(f: &Harness, name: &str) -> Result<PathBuf> {
    let key = f.private.join(name);
    f.quiet(
        f.o.tool("openssh").join("bin/ssh-keygen"),
        &["-q", "-t", "ed25519", "-N", "", "-f", &s(&key)],
    )?;
    Ok(key)
}
fn descendants(pid: u32) -> Result<Vec<u32>> {
    let mut result = vec![];
    for child in fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))?.split_whitespace()
    {
        let child = child.parse()?;
        result.extend(descendants(child)?);
        result.push(child);
    }
    Ok(result)
}
fn remote_command(f: &mut Harness, name: &str, marker: &str) -> Result<()> {
    let terminal = wait("actual SSH terminal", || {
        Ok(f.semantics()?
            .into_iter()
            .find(|row| row["role"] == "terminal"))
    })?;
    f.focus(&terminal)?;
    let keyboard = f.keyboard.as_mut().ok_or("Keyboard")?;
    keyboard.type_text(&format!("printf {marker} > /home/fixture/private/{name}"))?;
    keyboard.key(&["-k", "Return"])?;
    wait("remote SSH command receipt", || {
        Ok(f.private.join(name).exists().then_some(()))
    })?;
    ensure(
        fs::read_to_string(f.private.join(name))? == marker,
        "Remote command receipt differs",
    )
}
fn ssh(f: &mut Harness) -> Result<()> {
    fs::create_dir("/run/sshd")?;
    fs::create_dir_all("/var/empty")?;
    let host = sshkey(f, "host")?;
    let client = sshkey(f, "client")?;
    let wrong_key = sshkey(f, "wrong")?;
    let authorized = f.private.join("authorized_keys");
    fs::copy(client.with_extension("pub"), &authorized)?;
    fs::set_permissions(&authorized, fs::Permissions::from_mode(0o600))?;
    let wrong = f.o.scenario == "wrong-identity";
    let pinned = if wrong {
        sshkey(f, "different-host")?
    } else {
        host.clone()
    };
    let public = fs::read_to_string(pinned.with_extension("pub"))?;
    let fields = public.split_whitespace().collect::<Vec<_>>();
    let known = format!("[127.0.0.1]:2222 {} {}\n", fields[0], fields[1]);
    let ssh_home = PathBuf::from("/home/fixture/.ssh");
    fs::create_dir(&ssh_home)?;
    fs::set_permissions(&ssh_home, fs::Permissions::from_mode(0o700))?;
    fs::write(ssh_home.join("known_hosts"), &known)?;
    let config = f.private.join("sshd.conf");
    fs::write(
        &config,
        format!(
            "Port 2222\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}/sshd.pid\nAuthorizedKeysFile {}\nUsePAM no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nAuthenticationMethods publickey\nStrictModes yes\nAllowUsers fixture\nPermitTTY yes\nLogLevel VERBOSE\n",
            host.display(),
            f.private.display(),
            authorized.display()
        ),
    )?;
    let server = f.launch_s(
        f.o.tool("openssh").join("bin/sshd"),
        &["-D", "-e", "-f", &s(&config)],
        "sshd.log",
    )?;
    wait("actual SSH server", || {
        Ok(f.log("sshd.log")
            .contains("Server listening on 127.0.0.1 port 2222")
            .then_some(()))
    })?;
    let key = if f.o.scenario == "wrong-password" {
        wrong_key
    } else {
        client
    };
    fs::write(
        &f.profile,
        format!(
            "[remmina]\nname=Disposable SSH\nprotocol=SSH\nserver=127.0.0.1:2222\nusername=fixture\nssh_auth=1\nssh_privatekey={}\nviewmode=1\n",
            key.display()
        ),
    )?;
    f.connect()?;
    if wrong {
        wait("changed pre-pinned SSH identity warning", || {
            Ok(f.semantics()?
                .iter()
                .any(|row| {
                    row["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains("changed")
                        || row["description"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains("changed")
                })
                .then_some(()))
        })?;
        ensure(
            !f.log("sshd.log").contains("Accepted publickey"),
            "Changed SSH key authenticated",
        )?;
    } else if f.o.scenario == "wrong-password" {
        wait("real SSH wrong key rejection", || {
            Ok(f.log("sshd.log").contains("Failed publickey").then_some(()))
        })?;
        ensure(
            !f.log("sshd.log").contains("Accepted publickey"),
            "Wrong SSH identity authenticated",
        )?;
    } else {
        wait("actual Remmina SSH authentication", || {
            Ok(f.log("sshd.log")
                .contains("Accepted publickey")
                .then_some(()))
        })?;
        remote_command(f, "ssh-command-receipt", "lumen-native-ssh-receipt")?;
        let offset = f.log("remmina.log").len();
        let pid = f.children[server].id();
        let workers = descendants(pid)?;
        ensure(!workers.is_empty(), "No owned SSH connection workers")?;
        for worker in workers {
            unsafe {
                libc::kill(worker as i32, libc::SIGKILL);
            }
        }
        wait("SSH connection worker failure", || {
            let log = f.log("remmina.log");
            Ok(["Remove socket fd from event context", "Close SSH shell"]
                .iter()
                .any(|marker| log.get(offset..).unwrap_or("").contains(marker))
                .then_some(()))
        })?;
        let state = f.semantics()?;
        save(&f.out.join("network-loss-semantics.json"), &json!(state))?;
        f.client_stop()?;
        ensure(
            f.children[server].try_wait()?.is_none(),
            "SSH master stopped unexpectedly",
        )?;
        f.connect()?;
        wait("actual SSH same-profile reconnect", || {
            Ok((f.log("sshd.log").matches("Accepted publickey").count() == 2).then_some(()))
        })?;
        remote_command(f, "ssh-reconnect-receipt", "lumen-native-ssh-reconnected")?;
        ensure(
            fs::read_to_string(ssh_home.join("known_hosts"))? == known,
            "Pinned SSH identity changed",
        )?;
    }
    let state = f.semantics()?;
    save(&f.out.join("ssh-semantics.json"), &json!(state))?;
    f.capture("ssh.png")?;
    save(
        &f.out.join("acceptance.json"),
        &json!({"protocol":"SSH","scenario":f.o.scenario,"passed":true,"actualProvider":"Remmina","actualServer":"OpenSSH","authentication":"Ed25519 public key","hostIdentity":"pre-pinned known_hosts","physicalCommandExecuted":f.o.scenario=="authenticated","authenticated":f.o.scenario=="authenticated","transportLossObserved":f.o.scenario=="authenticated","reconnect":if f.o.scenario=="authenticated"{Some("manual same-profile client restart")}else{None}}),
    )
}
