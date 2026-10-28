use crate::harness::{Fixture, Options, Result, pause, require, wait_true};
use serde_json::{Value, json};
use std::fs;

fn find_name<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    if node["name"] == name {
        return Some(node);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                if let Some(found) = find_name(child, name) {
                    return Some(found);
                }
            }
        }
    }
    None
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options, include_str!("../interaction.qml"))?;
    f.start_sway()?;
    f.call(
        "dbus-update-activation-environment",
        &[
            "WAYLAND_DISPLAY",
            "XDG_CURRENT_DESKTOP",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
        ],
    )?;
    let qml = f.qml.display().to_string();
    f.launch("quickshell", &["-p", &qml], "dock", false)?;
    wait_true("Dock width", || {
        Ok(f.state()?["width"]
            .as_f64()
            .is_some_and(|value| value > 0.0))
    })?;
    pause(1000);
    f.start_pointer()?;
    let width = f.state()?["width"].as_f64().ok_or("Missing Dock width")?;
    let slot = (width - 24.0) / 8.0;
    let left = (1920.0 - width) / 2.0;
    let mut position = (960, 540);
    for held in [false, true] {
        f.move_to(
            &mut position,
            (left + 12.0 + slot / 2.0).round() as i64,
            1030,
        )?;
        if held {
            f.pointer("p\n")?;
        }
        for y in [1030, 975, 995, 1030] {
            for index in (0..8).chain((0..8).rev()) {
                f.move_to(
                    &mut position,
                    (left + 12.0 + (index as f64 + 0.5) * slot).round() as i64,
                    y,
                )?;
                require(
                    f.state()?["hovered"] == index,
                    format!("Dock hover: held={held} y={y} index={index}"),
                )?;
            }
        }
        f.move_to(&mut position, 20, 500)?;
        require(f.state()?["hovered"] == -1, "Dock hover did not clear")?;
        if held {
            f.pointer("r\n")?;
        }
    }
    f.screenshot("dock.png")?;
    let chooser = f.sibling("dock-chooser")?;
    f.launch(
        chooser.to_str().ok_or("Chooser path")?,
        &[],
        "chooser",
        false,
    )?;
    wait_true("GTK save dialog", || {
        Ok(find_name(&f.tree()?, "Save Dialog Fixture").is_some())
    })?;
    let tree = f.tree()?;
    let node = find_name(&tree, "Save Dialog Fixture").ok_or("Save dialog missing")?;
    require(
        node["border"] == "csd" || node["border"] == "none",
        "Compositor titlebar on GTK dialog",
    )?;
    require(
        node["deco_rect"]["height"] == 0,
        "Compositor decoration has height",
    )?;
    let rect = &node["rect"];
    require(
        rect["y"].as_i64().is_some_and(|value| value >= 0)
            && rect["height"].as_i64().is_some_and(|value| value <= 999),
        "GTK dialog out of bounds",
    )?;
    f.screenshot("chooser.png")?;
    fs::write(
        f.options.output.join("result.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"passed":true,"checks":["bidirectional Dock hover sweeps","held-button sweeps and exit clear","CSD save dialog has no compositor titlebar"]})
            )?
        ),
    )?;
    Ok(())
}
