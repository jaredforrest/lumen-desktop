//! Owned Files and Quick Look preview resource lifetime under real decoders.
use lumen_preview_input_fixtures::{
    Raster, Result, Session, copy_tree, ensure, fail, pause, s, sha_bytes, wait,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env, fs,
    io::{BufWriter, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

struct Opt {
    launcher: PathBuf,
    sway: PathBuf,
    output: PathBuf,
    effects: bool,
}
impl Opt {
    fn parse() -> Result<Self> {
        let mut launcher = None;
        let mut sway = None;
        let mut output = None;
        let mut effects = false;
        let mut a = env::args_os().skip(1);
        while let Some(v) = a.next() {
            match v.to_str() {
                Some("--launcher") => launcher = Some(PathBuf::from(a.next().ok_or("launcher")?)),
                Some("--sway") => sway = Some(PathBuf::from(a.next().ok_or("sway")?)),
                Some("--output") => output = Some(PathBuf::from(a.next().ok_or("output")?)),
                Some("--effects") => effects = true,
                _ => {
                    return Err(fail(format!(
                        "Unknown owned preview argument: {}",
                        v.to_string_lossy()
                    )));
                }
            }
        }
        Ok(Self {
            launcher: launcher.ok_or("launcher required")?,
            sway: sway.ok_or("sway required")?,
            output: output.ok_or("output required")?,
            effects,
        })
    }
}
fn image(path: &Path, width: i64, height: i64, rgb: [u8; 3]) -> Result<()> {
    Raster {
        width,
        height,
        pixels: rgb.repeat((width * height) as usize),
    }
    .save(path)
}
fn video(f: &Session, path: &Path, color: &str, duration: u32) -> Result<()> {
    f.call(
        "ffmpeg",
        &[
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=320x240:r=10"),
            "-t",
            &duration.to_string(),
            "-an",
            "-c:v",
            "ffv1",
            &s(path),
        ],
    )?;
    Ok(())
}
fn uri_path(uri: &str) -> PathBuf {
    PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri))
}
fn uri(path: &Path) -> String {
    format!("file://{}", s(path).replace(' ', "%20"))
}
fn stat_start(pid: u32) -> Result<String> {
    Ok(fs::read_to_string(format!("/proc/{pid}/stat"))?
        .rsplit_once(") ")
        .ok_or("stat")?
        .1
        .split_whitespace()
        .nth(19)
        .ok_or("start")?
        .to_owned())
}
fn find_node(tree: &Value, title: &str) -> Option<Value> {
    if tree["name"] == title && tree["app_id"] == "org.quickshell" {
        return Some(tree.clone());
    }
    for key in ["nodes", "floating_nodes"] {
        for child in tree[key].as_array().into_iter().flatten() {
            if let Some(n) = find_node(child, title) {
                return Some(n);
            }
        }
    }
    None
}
struct Test {
    f: Session,
    o: Opt,
    docs: PathBuf,
    qml: PathBuf,
    shell: u32,
    paused: HashMap<u32, String>,
    results: Vec<Value>,
    checks: Vec<&'static str>,
}
impl Drop for Test {
    fn drop(&mut self) {
        for (pid, start) in &self.paused {
            if stat_start(*pid).is_ok_and(|v| &v == start) {
                unsafe { libc::kill(*pid as i32, libc::SIGCONT) };
            }
        }
    }
}
impl Test {
    fn ipc(&self, method: &str, args: &[&str]) -> Result<String> {
        let mut cmd = vec![
            "-p",
            self.qml.to_str().ok_or("QML path")?,
            "ipc",
            "call",
            "previewAudit",
            method,
        ];
        cmd.extend(args);
        self.f.call("quickshell", &cmd)
    }
    fn state(&self, surface: &str) -> Result<Value> {
        Ok(serde_json::from_str(&self.ipc("status", &[surface])?)?)
    }
    fn wait_state(
        &self,
        surface: &str,
        label: &str,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Result<Value> {
        wait(label, 20, || {
            let v = self.state(surface)?;
            Ok(predicate(&v).then_some(v))
        })
    }
    fn call(&self, method: &str, args: &[&str]) -> Result<()> {
        self.ipc(method, args)?;
        Ok(())
    }
    fn key(&self, args: &[&str]) -> Result<()> {
        self.f.call("wtype", args)?;
        Ok(())
    }
    fn resource(&self, surface: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.state(surface)?["result"]["resourcePath"]
                .as_str()
                .ok_or("resource path")?,
        ))
    }
    fn focus(&self, title: &str) -> Result<Value> {
        let n = find_node(&self.f.tree()?, title).ok_or("window missing")?;
        self.f.call(
            "swaymsg",
            &[
                &format!("[con_id={}]", n["id"].as_i64().ok_or("con id")?),
                "focus",
            ],
        )?;
        Ok(n)
    }
    fn check(&mut self, v: &'static str) {
        self.checks.push(v)
    }
}
fn patch_preview(path: &Path) -> Result<(String, String)> {
    let original = fs::read_to_string(path)?;
    let header = "    property var result:";
    ensure(
        original.matches(header).count() == 1,
        "FilePreview result field changed",
    )?;
    let properties = "    property bool fixtureHold:true\n    property string fixturePinnedImage:\"\"\n    property string fixturePinnedMedia:\"\"\n    readonly property int fixtureImageStatus:fixtureImage.status\n    readonly property int fixtureDuration:player.duration\n    readonly property string fixtureSource:player.source.toString() || fixtureImage.source.toString()\n    readonly property bool fixturePlaying:player.playing\n    readonly property bool fixtureStopped:player.playbackState===MediaPlayer.StoppedState\n";
    let mut modified = original.replacen(header, &format!("{properties}{header}"), 1);
    let had_id = modified.contains("id:previewImage");
    if had_id {
        modified = modified
            .replacen("id:previewImage", "id:fixtureImage", 1)
            .replace("previewImage.source", "fixtureImage.source")
            .replace(
                "sourceObjects:[previewImage,player]",
                "sourceObjects:[fixtureImage,player]",
            )
    } else {
        modified = modified.replacen(
            "        Image {\n",
            "        Image {\n            id:fixtureImage\n",
            1,
        )
    }
    modified = modified
        .replace(
            "root.active && picture.visible",
            "root.active && !root.fixtureHold && picture.visible",
        )
        .replace(
            "root.active && root.media",
            "root.active && !root.fixtureHold && root.media",
        );
    let specs = [
        (
            "Image",
            "root.active && !root.fixtureHold && picture.visible ? root.result.uri || \"\" : \"\"",
        ),
        (
            "Media",
            "root.active && !root.fixtureHold && root.media ? root.result.uri || \"\" : \"\"",
        ),
    ];
    for (name, expr) in specs {
        let needle = format!("source: {expr}");
        ensure(
            modified.matches(&needle).count() == 1,
            format!("Missing {name} source expression"),
        )?;
        modified = modified.replacen(
            &needle,
            &format!("source: root.fixturePinned{name} || ({expr})"),
            1,
        )
    }
    ensure(
        modified.matches("!root.fixtureHold").count() == 2,
        "Preview hold probes changed",
    )?;
    let mut roundtrip = modified.clone();
    for (name, expr) in specs {
        roundtrip = roundtrip.replacen(
            &format!("source: root.fixturePinned{name} || ({expr})"),
            &format!("source: {expr}"),
            1,
        )
    }
    roundtrip = roundtrip
        .replace(
            "root.active && !root.fixtureHold && picture.visible",
            "root.active && picture.visible",
        )
        .replace(
            "root.active && !root.fixtureHold && root.media",
            "root.active && root.media",
        )
        .replacen(properties, "", 1);
    if had_id {
        roundtrip = roundtrip
            .replacen("id:fixtureImage", "id:previewImage", 1)
            .replace("fixtureImage.source", "previewImage.source")
            .replace(
                "sourceObjects:[fixtureImage,player]",
                "sourceObjects:[previewImage,player]",
            )
    } else {
        roundtrip = roundtrip.replacen("            id:fixtureImage\n", "", 1)
    }
    ensure(roundtrip == original, "FilePreview probe roundtrip")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::write(path, &modified)?;
    Ok((original, modified))
}
fn setup(o: Opt) -> Result<Test> {
    let mut f = Session::new(&o.output, &o.launcher, &o.sway, "lumen-owned-preview-")?;
    f.set_many(&[
        ("QT_QPA_PLATFORM", "wayland"),
        ("QT_QPA_PLATFORMTHEME", "generic"),
        ("QT_QUICK_BACKEND", "software"),
        ("QT_ACCESSIBILITY", "0"),
        ("QT_LINUX_ACCESSIBILITY_ALWAYS_ON", "0"),
        ("NO_AT_BRIDGE", "1"),
        ("GTK_A11Y", "none"),
        ("WLR_BACKENDS", "headless"),
        ("WLR_RENDERER", if o.effects { "gles2" } else { "pixman" }),
        ("WLR_RENDERER_ALLOW_SOFTWARE", "1"),
        (
            "WLR_RENDERER_FORCE_SOFTWARE",
            if o.effects { "1" } else { "0" },
        ),
        ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ("WLR_LIBINPUT_NO_DEVICES", "1"),
        ("GLOBAL_MENU_ENABLED", "0"),
        ("LUMEN_FILE_SERVICE", "0"),
        ("LUMEN_LAUNCH_SCOPES", "0"),
    ]);
    f.start_bus()?;
    let out = f.out.clone();
    let assets = out.join("assets");
    let installed = PathBuf::from(
        f.env
            .get("LUMEN_DESKTOP_ASSETS")
            .ok_or("Installed assets missing")?,
    );
    copy_tree(&installed, &assets)?;
    let (original, modified) = patch_preview(&assets.join("shell/FilePreview.qml"))?;
    let state = assets.join("shell/OwnedPreviewState.qml");
    let (state_original, state_modified) = if state.exists() {
        let old = fs::read_to_string(&state)?;
        let new=old.replace("    objectName:\"owned-preview-state\"","    objectName:\"owned-preview-state\"\n    property var fixtureAccepted:[]").replace("root.result=data;root.received(data);","root.fixtureAccepted=root.fixtureAccepted.concat([{id:data.id,path:data.sourcePath,token:data.sourceToken,duration:data.duration}]);root.result=data;root.received(data);");
        ensure(new != old, "Owned state probe markers changed")?;
        fs::set_permissions(&state, fs::Permissions::from_mode(0o600))?;
        fs::write(&state, &new)?;
        (Some(old), Some(new))
    } else {
        (None, None)
    };
    fs::write(
        out.join("instrumentation.json"),
        serde_json::to_vec_pretty(
            &json!({"launcher":s(&o.launcher),"originalSha256":sha_bytes(original.as_bytes()),"modifiedSha256":sha_bytes(modified.as_bytes()),"stateOriginalSha256":state_original.as_ref().map(|v|sha_bytes(v.as_bytes())),"stateModifiedSha256":state_modified.as_ref().map(|v|sha_bytes(v.as_bytes())),"roundtrip":true,"scope":"Hash-verified reversible actual Image/MediaPlayer activation and source-retirement gates; read-only status/accepted reply log. Provider, request, result and URI unchanged."}),
        )?,
    )?;
    let palette: Value =
        serde_json::from_slice(&fs::read(assets.join("appearance/palette.json"))?)?;
    let appearance = PathBuf::from(f.env.get("XDG_STATE_HOME").ok_or("State home")?)
        .join("lumen-desktop/appearance.json");
    fs::create_dir_all(appearance.parent().ok_or("appearance parent")?)?;
    fs::write(
        &appearance,
        serde_json::to_vec(
            &json!({"font":palette["font"],"palette":palette["dark"],"preferences":{}}),
        )?,
    )?;
    let docs = f.root.path().join("Synthetic");
    fs::create_dir(&docs)?;
    image(&docs.join("image.png"), 320, 240, [255, 0, 0])?;
    image(&docs.join("replacement.png"), 320, 240, [0, 0, 255])?;
    video(&f, &docs.join("video.mkv"), "red", 2)?;
    video(&f, &docs.join("replacement.mkv"), "blue", 6)?;
    let qml = out.join("shell.qml");
    fs::write(
        &qml,
        include_str!("../../owned.qml")
            .replace("__DOCS_JSON__", &serde_json::to_string(&s(&docs))?),
    )?;
    let conf = out.join("sway.conf");
    fs::write(
        &conf,
        "output HEADLESS-1 resolution 1280x1000\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n",
    )?;
    f.launch(
        &s(&o.sway),
        &["--unsupported-gpu", "-c", &s(&conf)],
        "sway.log",
        false,
    )?;
    f.wait_sway(1)?;
    f.launch("wtype", &["-s", "60000"], "keyboard.log", false)?;
    let shell = f.launch("quickshell", &["-p", &s(&qml)], "shell.log", false)?;
    let shell_pid = f.child_pid(shell);
    let t = Test {
        f,
        o,
        docs,
        qml,
        shell: shell_pid,
        paused: HashMap::new(),
        results: Vec::new(),
        checks: Vec::new(),
    };
    t.wait_state("files", "Files ready", |v| v["ready"] == true)?;
    Ok(t)
}
fn run(t: &mut Test) -> Result<()> {
    let out = t.f.out.clone();
    for surface in ["files", "quick"] {
        for kind in ["image", "media"] {
            let target = t.docs.join(if kind == "image" {
                "image.png"
            } else {
                "video.mkv"
            });
            if kind == "image" {
                image(&target, 320, 240, [255, 0, 0])?
            } else {
                fs::remove_file(&target)?;
                video(&t.f, &target, "red", 2)?
            }
            t.call("refreshRows", &[])?;
            t.wait_state("files", "current directory identity", |v| {
                v["ready"] == true
                    && v["rows"].as_array().is_some_and(|a| {
                        a.iter().any(|r| {
                            r["path"] == s(&target)
                                && r["identity"]["inode"].as_str()
                                    == fs::metadata(&target)
                                        .ok()
                                        .map(|m| m.ino().to_string())
                                        .as_deref()
                        })
                    })
            })?;
            pause(400);
            t.call("startPreview", &[surface, &s(&target)])?;
            let before =
                t.wait_state(surface, &format!("{surface} {kind} original reply"), |v| {
                    v["result"]["type"] == kind && v["held"] == true
                })?;
            ensure(before["source"] == "", "Held preview had source")?;
            let old = fs::metadata(&target)?;
            let mut paused = None;
            if surface == "files" {
                let children =
                    fs::read_to_string(format!("/proc/{}/task/{}/children", t.shell, t.shell))?;
                let helper = PathBuf::from(t.f.env.get("LUMEN_FILES_BIN").ok_or("Files helper")?)
                    .file_name()
                    .ok_or("helper name")?
                    .to_string_lossy()
                    .into_owned();
                let pid = children
                    .split_whitespace()
                    .filter_map(|v| v.parse::<u32>().ok())
                    .find(|pid| {
                        fs::read(format!("/proc/{pid}/cmdline"))
                            .is_ok_and(|b| b.windows(helper.len()).any(|v| v == helper.as_bytes()))
                    })
                    .ok_or("Owned preview Files PID")?;
                let start = stat_start(pid)?;
                t.paused.insert(pid, start);
                unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
                paused = Some(pid)
            }
            let replacement = t.docs.join(if kind == "image" {
                "replacement.png"
            } else {
                "replacement.mkv"
            });
            if kind == "image" {
                image(&replacement, 320, 240, [0, 0, 255])?
            } else {
                let _ = fs::remove_file(&replacement);
                video(&t.f, &replacement, "blue", 6)?
            }
            fs::rename(&replacement, &target)?;
            let new = fs::metadata(&target)?;
            ensure(
                (old.dev(), old.ino()) != (new.dev(), new.ino()),
                "File identity did not change",
            )?;
            t.call("release", &[surface])?;
            let after = t.wait_state(surface, &format!("{surface} {kind} delayed load"), |v| {
                v["source"].as_str().is_some_and(|s| !s.is_empty())
                    && (if kind == "image" {
                        v["imageStatus"] == 1
                    } else {
                        v["duration"].as_i64().unwrap_or(0) > 0
                    })
            })?;
            pause(150);
            let screenshot = out.join(format!("{surface}-{kind}.png"));
            t.f.capture(&screenshot, &[])?;
            let mut pixels = Value::Null;
            if kind == "image" {
                let title = after["geometry"]["title"]
                    .as_str()
                    .ok_or("geometry title")?;
                let node = find_node(&t.f.tree()?, title).ok_or("Preview window")?;
                let g = &after["geometry"];
                let r = &node["rect"];
                let x = r["x"].as_i64().unwrap_or(0) + g["x"].as_i64().unwrap_or(0);
                let y = r["y"].as_i64().unwrap_or(0) + g["y"].as_i64().unwrap_or(0);
                let w = g["width"].as_i64().ok_or("preview width")?;
                let h = g["height"].as_i64().ok_or("preview height")?;
                let crop = Raster::read(&screenshot)?.crop(x + 8, y + 8, x + w - 8, y + h - 42);
                let mut red = 0;
                let mut blue = 0;
                for p in crop.pixels.chunks_exact(3) {
                    if p[0] > 200 && p[1] < 40 && p[2] < 40 {
                        red += 1
                    }
                    if p[2] > 200 && p[0] < 40 && p[1] < 40 {
                        blue += 1
                    }
                }
                pixels = json!({"red":red,"blue":blue})
            }
            t.results.push(json!({"surface":surface,"kind":kind,"oldIdentity":[old.dev(),old.ino()],"newIdentity":[new.dev(),new.ino()],"before":before,"after":after,"pixels":pixels}));
            if let Some(pid) = paused {
                unsafe { libc::kill(pid as i32, libc::SIGCONT) };
                t.paused.remove(&pid);
            }
        }
    }
    fs::write(
        out.join("baseline.json"),
        serde_json::to_vec_pretty(&t.results)?,
    )?;
    for result in &t.results {
        if result["kind"] == "image" {
            ensure(
                result["pixels"]["blue"].as_i64().unwrap_or(0)
                    <= result["pixels"]["red"].as_i64().unwrap_or(0),
                "Owned preview reopened blue replacement image",
            )?
        } else {
            let duration = result["after"]["duration"].as_i64().unwrap_or(0);
            let expected = result["before"]["result"]["duration"].as_i64().unwrap_or(0) * 1000;
            ensure(
                (duration - expected).abs() <= 200,
                "Owned preview reopened six-second replacement media",
            )?
        }
    }
    t.check("Files and Quick Look image pixels retain original red after delayed load of replaced blue path");
    t.check("Files and Quick Look actual media decoder retains original two-second resource after six-second replacement");
    continue_checks(t)?;
    Ok(())
}
fn continue_checks(t: &mut Test) -> Result<()> {
    let out = t.f.out.clone();
    let quick_resource = PathBuf::from(
        t.results.last().ok_or("quick resource")?["before"]["result"]["resourcePath"]
            .as_str()
            .ok_or("quick resource path")?,
    );
    let obsolete = PathBuf::from(
        t.results[1]["before"]["result"]["resourcePath"]
            .as_str()
            .ok_or("obsolete Files resource")?,
    );
    let target = t.docs.join("image.png");
    t.call("refreshRows", &[])?;
    t.wait_state("files", "current Files rows", |v| v["ready"] == true)?;
    t.call("restartFilesPreview", &[&s(&target)])?;
    t.wait_state("files", "current Files independent lease", |v| {
        v["result"]["type"] == "image" && v["result"]["sourcePath"] == s(&target)
    })?;
    t.call("release", &["files"])?;
    let current = t.wait_state("files", "current Files source loaded", |v| {
        v["imageStatus"] == 1 && v["source"] == v["result"]["uri"]
    })?;
    let files_resource = PathBuf::from(
        current["result"]["resourcePath"]
            .as_str()
            .ok_or("Files resource")?,
    );
    ensure(
        files_resource != obsolete,
        "Files resource reused retired lease",
    )?;
    wait("old unselected Files lease retired", 20, || {
        Ok((!obsolete.exists()).then_some(()))
    })?;
    t.focus("Quick Look")?;
    t.key(&["-k", "Escape"])?;
    t.wait_state("quick", "utility keyboard close", |v| {
        v["visible"] == false && v["source"] == ""
    })?;
    wait("utility resource retirement", 20, || {
        Ok((!quick_resource.exists()).then_some(()))
    })?;
    let after_files = t.state("files")?;
    fs::write(
        out.join("owner-debug.json"),
        serde_json::to_vec_pretty(
            &json!({"before":current,"after":after_files,"expected":s(&files_resource)}),
        )?,
    )?;
    ensure(
        files_resource.exists(),
        "Utility cleanup removed another live resource",
    )?;
    ensure(
        after_files["source"] == current["source"]
            && after_files["result"]["sourceToken"] == current["result"]["sourceToken"],
        "Utility changed Files token",
    )?;
    fs::write(
        out.join("independent-owner.json"),
        serde_json::to_vec_pretty(
            &json!({"before":current,"after":after_files,"obsoleteRetired":s(&obsolete)}),
        )?,
    )?;
    t.check("Physical Escape retires utility sources/resources while preserving exact current Files source and token; old unselected lease retires");
    image(&target, 320, 240, [255, 0, 0])?;
    t.call("refreshRows", &[])?;
    t.wait_state("files", "directory refresh", |v| v["ready"] == true)?;
    t.call("startPreview", &["quick", &s(&target)])?;
    let fresh = t.wait_state("quick", "fresh utility", |v| v["result"]["type"] == "image")?;
    let fresh_resource = PathBuf::from(
        fresh["result"]["resourcePath"]
            .as_str()
            .ok_or("fresh resource")?,
    );
    ensure(
        fresh_resource != quick_resource && fresh_resource.exists(),
        "Fresh utility reused retired lease",
    )?;
    t.call("release", &["quick"])?;
    t.wait_state("quick", "fresh utility image", |v| v["imageStatus"] == 1)?;
    t.check("Reopened utility receives a fresh observed snapshot and provider lifetime");
    let (files_pid, files_start) = resource_pid(&files_resource)?;
    ensure(
        stat_start(files_pid)? == files_start,
        "Files preview process identity changed",
    )?;
    unsafe { libc::kill(files_pid as i32, libc::SIGKILL) };
    t.wait_state("files", "preview provider exit", |v| {
        v["result"]["error"].as_str().is_some_and(|s| !s.is_empty())
            || v["result"].as_object().is_some_and(|o| o.is_empty())
    })?;
    ensure(
        t.state("files")?["ready"] == true,
        "Preview crash stopped directory backend",
    )?;
    t.call("restartFilesPreview", &[&s(&target)])?;
    t.wait_state("files", "Files preview restart", |v| {
        v["result"]["type"] == "image"
    })?;
    t.call("release", &["files"])?;
    t.wait_state("files", "Files recovered image", |v| v["imageStatus"] == 1)?;
    wait("orphaned resource cleanup", 20, || {
        Ok((!files_resource.exists()).then_some(()))
    })?;
    ensure(
        fresh_resource.exists(),
        "New Files provider removed live utility resource",
    )?;
    t.check("Preview crash leaves directory backend usable and reconnect cleans only dead-owner resources");
    let (quick_pid, quick_start) = resource_pid(&fresh_resource)?;
    ensure(
        stat_start(quick_pid)? == quick_start,
        "Quick preview process identity changed",
    )?;
    t.paused.insert(quick_pid, quick_start);
    unsafe { libc::kill(quick_pid as i32, libc::SIGSTOP) };
    t.call("startPreview", &["quick", &s(&t.docs.join("video.mkv"))])?;
    t.wait_state("quick", "stopped preview request", |v| {
        v["controller"]["busy"] == true
    })?;
    t.call("focusFiles", &[])?;
    t.key(&["-M", "ctrl", "-k", "2", "-m", "ctrl"])?;
    t.call("refreshRows", &[])?;
    t.wait_state("files", "folder controls while preview stopped", |v| {
        v["ready"] == true
    })?;
    t.wait_state("quick", "preview deadline", |v| {
        v["result"]["error"].as_str().is_some_and(|s| !s.is_empty())
            && v["controller"]["busy"] == false
    })?;
    wait("owned stalled preview termination", 20, || {
        Ok((!PathBuf::from(format!("/proc/{quick_pid}")).exists()).then_some(()))
    })?;
    t.paused.remove(&quick_pid);
    ensure(
        t.state("files")?["ready"] == true && t.state("quick")?["source"] == "",
        "Stopped preview affected folder or revived source",
    )?;
    t.check("Actual SIGSTOP preview reaches bounded unavailable state without stopping folder controls or retrying stale reads");
    let large = t.docs.join("large.mkv");
    fs::File::create(&large)?.set_len(64 * 1024 * 1024 + 1)?;
    t.call("startPreview", &["quick", &s(&large)])?;
    t.wait_state("quick", "oversize refusal", |v| {
        v["result"]["error"].as_str().is_some_and(|s| !s.is_empty())
    })?;
    ensure(
        t.state("quick")?["source"] == ""
            && t.state("quick")?["result"]["uri"]
                .as_str()
                .is_none_or(|s| s.is_empty()),
        "Oversize used mutable URI",
    )?;
    t.check("Oversized resources fail without mutable original-URI fallback");
    pdf_checks(t)?;
    Ok(())
}
fn resource_pid(path: &Path) -> Result<(u32, String)> {
    let folder = path
        .parent()
        .ok_or("resource parent")?
        .file_name()
        .ok_or("resource folder")?
        .to_string_lossy();
    let mut parts = folder.split('-');
    Ok((
        parts.next().ok_or("PID")?.parse()?,
        parts.next().ok_or("start token")?.to_owned(),
    ))
}
fn pdf(path: &Path) -> Result<Vec<u8>> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".to_vec(),
    ];
    for (index, color) in ["1 0 0", "0 1 0", "0 0 1"].iter().enumerate() {
        let drawing = format!("{color} rg 0 0 320 240 re f");
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 240] /Contents {} 0 R /Resources << >> >>",4+index*2).into_bytes());
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{drawing}\nendstream",
                drawing.len()
            )
            .into_bytes(),
        )
    }
    let mut content = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(content.len());
        content.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        content.extend_from_slice(object);
        content.extend_from_slice(b"\nendobj\n")
    }
    let xref = content.len();
    content.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        content.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes())
    }
    content.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(path, &content)?;
    Ok(content)
}
fn bmp(path: &Path, rgb: [u8; 3]) -> Result<()> {
    let width = 4096u32;
    let height = 2560u32;
    let bytes = width * height * 3;
    let mut header = [0u8; 54];
    header[0..2].copy_from_slice(b"BM");
    header[2..6].copy_from_slice(&(54 + bytes).to_le_bytes());
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    header[18..22].copy_from_slice(&width.to_le_bytes());
    header[22..26].copy_from_slice(&height.to_le_bytes());
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    header[28..30].copy_from_slice(&24u16.to_le_bytes());
    header[34..38].copy_from_slice(&bytes.to_le_bytes());
    let mut file = BufWriter::new(fs::File::create(path)?);
    file.write_all(&header)?;
    let row = [rgb[2], rgb[1], rgb[0]].repeat(width as usize);
    for _ in 0..height {
        file.write_all(&row)?
    }
    file.flush()?;
    Ok(())
}
fn bmp_pixel(path: &Path) -> Result<[u8; 3]> {
    let data = fs::read(path)?;
    ensure(
        data.starts_with(b"BM") && data.len() > 57,
        "Snapshot is not BMP",
    )?;
    Ok([data[56], data[55], data[54]])
}
fn pdf_checks(t: &mut Test) -> Result<()> {
    let out = t.f.out.clone();
    let document = t.docs.join("document.pdf");
    let bytes = pdf(&document)?;
    t.call("startPreview", &["quick", &s(&document)])?;
    let page = t.wait_state("quick", "rendered PDF reply", |v| {
        v["result"]["type"] == "image"
    })?;
    let page_resource = uri_path(page["result"]["uri"].as_str().ok_or("PDF URI")?);
    ensure(
        s(&page_resource).contains("lumen-preview-snapshots-"),
        "PDF page not snapshotted",
    )?;
    let cache = PathBuf::from(t.f.env.get("XDG_CACHE_HOME").ok_or("cache home")?)
        .join("lumen-desktop/previews");
    if cache.exists() {
        for e in fs::read_dir(cache)? {
            let p = e?.path();
            if p.extension().is_some_and(|v| v == "png") {
                fs::remove_file(p)?
            }
        }
    }
    ensure(
        page_resource.exists(),
        "PDF page did not survive cache prune",
    )?;
    t.call("release", &["quick"])?;
    t.wait_state("quick", "retained PDF page after cache prune", |v| {
        v["imageStatus"] == 1
    })?;
    t.check("Actual PDF raster remains available after shared renderer cache pruning");
    let input = PathBuf::from(page["result"]["resourcePath"].as_str().ok_or("PDF input")?);
    let token = page["result"]["sourceToken"].clone();
    let mut uris = vec![page_resource];
    t.focus("Quick Look")?;
    let mut proof = Vec::<Value>::new();
    for number in [2, 3] {
        t.key(&["-k", "Down"])?;
        let v = t.wait_state("quick", &format!("physical PDF page {number}"), |v| {
            v["result"]["requestedPage"] == number && v["imageStatus"] == 1
        })?;
        let raster = uri_path(v["result"]["uri"].as_str().ok_or("page URI")?);
        ensure(
            !uris.contains(&raster) && raster.exists() && input.exists(),
            "PDF paging reused or lost resource",
        )?;
        ensure(
            v["result"]["resourcePath"] == s(&input) && v["result"]["sourceToken"] == token,
            "PDF input/token changed on paging",
        )?;
        wait("previous PDF rasters retired", 20, || {
            Ok(uris.iter().all(|p| !p.exists()).then_some(()))
        })?;
        uris.push(raster);
        proof.push(v)
    }
    fs::write(
        out.join("pdf-leases.json"),
        serde_json::to_vec_pretty(&json!({"input":s(&input),"pages":proof}))?,
    )?;
    t.check("Physical PDF paging reuses exact immutable input and retires previous page rasters without URI reuse");
    let replacement = t.docs.join("replacement-document.pdf");
    fs::write(&replacement, &bytes)?;
    fs::rename(replacement, &document)?;
    t.key(&["-k", "Up"])?;
    t.wait_state("quick", "changed original PDF token refused", |v| {
        v["result"]["error"].as_str().is_some_and(|s| !s.is_empty()) && v["source"] == ""
    })?;
    t.check("Paging after original PDF replacement remains explicitly unavailable rather than adopting a replacement");
    gallery_checks(t)?;
    Ok(())
}
fn gallery_checks(t: &mut Test) -> Result<()> {
    let out = t.f.out.clone();
    let colors = [
        ("red", [255, 0, 0]),
        ("green", [0, 128, 0]),
        ("blue", [0, 0, 255]),
        ("yellow", [255, 255, 0]),
        ("magenta", [255, 0, 255]),
    ];
    let mut large = Vec::<(PathBuf, &str, [u8; 3])>::new();
    for (i, (name, rgb)) in colors.iter().enumerate() {
        let path = t.docs.join(format!("working-{i}.bmp"));
        bmp(&path, *rgb)?;
        large.push((path, *name, *rgb))
    }
    let sum = large
        .iter()
        .map(|(p, _, _)| fs::metadata(p).map(|m| m.len()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .sum::<u64>();
    ensure(
        sum > 128 * 1024 * 1024,
        "Gallery inputs below lifetime budget",
    )?;
    t.call("refreshRows", &[])?;
    t.wait_state("files", "large gallery rows", |v| {
        v["ready"] == true
            && large.iter().all(|(p, _, _)| {
                v["rows"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|r| r["path"] == s(p)))
            })
    })?;
    t.call("focusFiles", &[])?;
    let title = t.state("files")?["geometry"]["title"]
        .as_str()
        .ok_or("Files title")?
        .to_owned();
    t.focus(&title)?;
    t.key(&["-M", "ctrl", "-k", "4", "-m", "ctrl"])?;
    t.wait_state("files", "actual Files gallery shortcut", |v| {
        v["viewMode"] == "gallery"
    })?;
    let mut proof = Vec::<Value>::new();
    for surface in ["files", "quick"] {
        let mut previous = Vec::<PathBuf>::new();
        for (path, color, rgb) in &large {
            let accepted = t.state(surface)?["controller"]["accepted"]
                .as_array()
                .ok_or("accepted log")?
                .len();
            t.call("startPreview", &[surface, &s(path)])?;
            let v = t.wait_state(
                surface,
                &format!("{surface} gallery resource {color}"),
                |v| {
                    v["result"]["sourcePath"] == s(path)
                        && v["result"]["type"] == "image"
                        && v["controller"]["accepted"]
                            .as_array()
                            .is_some_and(|a| a.len() > accepted)
                },
            )?;
            let resource = uri_path(v["result"]["uri"].as_str().ok_or("gallery URI")?);
            ensure(
                !previous.contains(&resource) && resource.exists(),
                "Gallery reused URI",
            )?;
            t.call("release", &[surface])?;
            let loaded = t.wait_state(
                surface,
                &format!("{surface} decoded gallery {color}"),
                |v| v["imageStatus"] == 1 && v["source"] == uri(&resource),
            )?;
            wait("previous gallery leases retired", 20, || {
                Ok(previous.iter().all(|p| !p.exists()).then_some(()))
            })?;
            previous.push(resource.clone());
            ensure(
                bmp_pixel(&resource)? == *rgb,
                "Gallery snapshot color changed",
            )?;
            let live = fs::read_dir(resource.parent().ok_or("resource parent")?)?
                .filter_map(|e| e.ok().map(|v| v.path()))
                .collect::<Vec<_>>();
            let bytes = live
                .iter()
                .map(|p| fs::metadata(p).map(|m| m.len()))
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .sum::<u64>();
            ensure(
                live.len() == 1 && bytes <= 64 * 1024 * 1024,
                "Gallery working set exceeded bound",
            )?;
            proof.push(json!({"surface":surface,"path":s(path),"source":s(&resource),"pixel":rgb,"workingBytes":bytes,"state":loaded}))
        }
    }
    fs::write(
        out.join("gallery-leases.json"),
        serde_json::to_vec_pretty(&proof)?,
    )?;
    t.check("Files and Quick Look decode more than128 MiB of successive gallery inputs with one bounded current resource");
    let pinned = t.state("quick")?;
    let old = uri_path(pinned["result"]["uri"].as_str().ok_or("pinned image URI")?);
    let accepted = pinned["controller"]["accepted"]
        .as_array()
        .ok_or("accepted log")?
        .len();
    t.call("pinRetirement", &["quick"])?;
    t.call("startPreview", &["quick", &s(&large[0].0)])?;
    t.wait_state("quick", "held actual Image retirement", |v| {
        v["controller"]["resourceRetirement"] == true
    })?;
    pause(200);
    let held = t.state("quick")?;
    ensure(
        old.exists()
            && held["source"] == uri(&old)
            && held["controller"]["accepted"]
                .as_array()
                .is_some_and(|a| a.len() == accepted),
        "Image lease released before acknowledgement",
    )?;
    t.call("unpinRetirement", &["quick"])?;
    t.wait_state("quick", "retired Image replacement reply", |v| {
        v["result"]["sourcePath"] == s(&large[0].0)
    })?;
    wait("held Image lease released", 20, || {
        Ok((!old.exists()).then_some(()))
    })?;
    t.call("release", &["quick"])?;
    t.wait_state("quick", "replacement image decoded", |v| {
        v["imageStatus"] == 1
    })?;
    let image_retirement = json!({"before":pinned,"held":held,"after":t.state("quick")?,"retiredUri":s(&old),"retiredFileExists":old.exists()});
    t.check("Held actual Image source keeps old URI pinned and blocks replacement allocation until acknowledged retirement");
    let playing_path = t.docs.join("playing.mkv");
    video(&t.f, &playing_path, "red", 12)?;
    t.call("startPreview", &["quick", &s(&playing_path)])?;
    t.wait_state("quick", "playing media response", |v| {
        v["result"]["type"] == "media"
    })?;
    t.call("release", &["quick"])?;
    t.wait_state("quick", "actual media decoder ready", |v| {
        v["duration"].as_i64().unwrap_or(0) > 10000
    })?;
    t.call("play", &[])?;
    let playing = t.wait_state("quick", "actual MediaPlayer playback", |v| {
        v["playing"] == true
    })?;
    let old_media = uri_path(playing["result"]["uri"].as_str().ok_or("media URI")?);
    let accepted = playing["controller"]["accepted"]
        .as_array()
        .ok_or("accepted log")?
        .len();
    t.call("pinRetirement", &["quick"])?;
    t.call("startPreview", &["quick", &s(&large[1].0)])?;
    t.wait_state("quick", "held actual MediaPlayer retirement", |v| {
        v["controller"]["resourceRetirement"] == true
    })?;
    pause(200);
    let held = t.state("quick")?;
    ensure(
        old_media.exists()
            && held["source"] == uri(&old_media)
            && held["controller"]["accepted"]
                .as_array()
                .is_some_and(|a| a.len() == accepted),
        "Playing media lease released early",
    )?;
    t.call("unpinRetirement", &["quick"])?;
    t.wait_state("quick", "retired media replacement reply", |v| {
        v["result"]["sourcePath"] == s(&large[1].0)
    })?;
    t.wait_state(
        "quick",
        "held MediaPlayer stopped and lease released",
        |v| !old_media.exists() && v["stopped"] == true,
    )?;
    let media_retirement = json!({"before":playing,"held":held,"after":t.state("quick")?,"retiredUri":s(&old_media),"retiredFileExists":old_media.exists()});
    fs::write(
        out.join("retirement-leases.json"),
        serde_json::to_vec_pretty(&json!({"image":image_retirement,"media":media_retirement}))?,
    )?;
    t.check("Actual playing MediaPlayer source is retained until source-cleared and stopped acknowledgement before replacement");
    let log = fs::read_to_string(out.join("shell.log"))?;
    for marker in [
        "ReferenceError:",
        "TypeError:",
        "Binding loop",
        "Failed to load configuration",
        "Cannot assign",
    ] {
        ensure(!log.contains(marker), format!("QML error: {marker}"))?
    }
    ensure(t.checks.len() == 13, "Owned preview check count changed")?;
    fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"passed":true,"checks":t.checks,"resources":t.results,"final":t.state("quick")?}),
        )?,
    )?;
    Ok(())
}
fn main() {
    let result = (|| -> Result<()> {
        let mut t = setup(Opt::parse()?)?;
        run(&mut t)
    })();
    if let Err(e) = result {
        eprintln!("Owned preview fixture: {e}");
        std::process::exit(1)
    }
}
