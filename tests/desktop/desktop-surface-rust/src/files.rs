use crate::harness::{Fixture, Options, Result, fail, pause, require, save, wait_true};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| fail("Missing numeric fixture state"))
}
fn rows(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().ok_or_else(|| fail("Missing fixture rows"))
}
fn state(f: &Fixture, qml: &Path) -> Result<Value> {
    f.state(qml, "desktopFilesTest")
}
fn ipc(f: &Fixture, qml: &Path, method: &str, args: &[&str]) -> Result<String> {
    f.ipc(qml, "desktopFilesTest", method, args)
}
fn gesture(f: &mut Fixture, commands: &[(&str, u64)]) -> Result<()> {
    for (command, delay) in commands {
        f.pointer(&format!("{command}\n"))?;
        pause(*delay);
    }
    Ok(())
}
fn has_position(value: &Value) -> bool {
    value["layout"]["positions"]
        .as_object()
        .is_some_and(|map| !map.is_empty())
}
fn stack(value: &Value) -> Result<&Value> {
    rows(&value["items"])?
        .iter()
        .find(|item| item["stack"] == true)
        .ok_or_else(|| fail("Missing stack"))
}
fn at<'a>(value: &'a Value, index: usize) -> Result<&'a Value> {
    rows(value)?
        .get(index)
        .ok_or_else(|| fail("Missing desktop item"))
}
pub fn run(options: Options) -> Result<()> {
    let mut f = Fixture::new(options)?;
    let folder = f.desktop.join("Folder");
    fs::create_dir(&folder)?;
    let one = f.desktop.join("one.txt");
    fs::write(&one, "Disposable desktop fixture")?;
    let qml = f.options.output.join("fixture.qml");
    fs::write(
        &qml,
        include_str!("../files.qml")
            .replace("@ASSETS@", &f.assets.display().to_string())
            .replace(
                "@LAYER@",
                if f.options.layer == "top" {
                    "Top"
                } else {
                    "Bottom"
                },
            ),
    )?;
    f.start(
        "output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\n",
        "30000",
    )?;
    f.launch(
        "quickshell",
        &["-p", qml.to_str().ok_or("QML path")?],
        "shell",
        false,
    )?;
    wait_true("Desktop Files rows", || Ok(state(&f, &qml)?["count"] == 2))?;
    require(
        state(&f, &qml)?["path"] == f.desktop.display().to_string()
            && state(&f, &qml)?["error"] == "",
        "Wrong Desktop directory or error",
    )?;
    f.screenshot("desktop.png", false)?;
    f.cursor(1360, 180)?;
    gesture(&mut f, &[("m 10 10", 150), ("p", 150), ("r", 300)])?;
    f.screenshot("clicked.png", true)?;
    let click = json!({"state":state(&f,&qml)?,"seats":serde_json::from_str::<Value>(&f.sway(&["-t","get_seats","-r"])?)?,"inputs":serde_json::from_str::<Value>(&f.sway(&["-t","get_inputs","-r"])?)?,"pointer_exit":null});
    save(&f.options.output.join("click-state.json"), &click)?;
    wait_true("Desktop selection", || {
        Ok(state(&f, &qml)?["selected"] == json!([one.display().to_string()]))
    })?;
    let mut dragged = Value::Null;
    if f.options.layout {
        pause(700);
        let before = at(&state(&f, &qml)?["items"], 1)?.clone();
        require(
            before["row"]["name"] == "one.txt",
            "Wrong initial desktop row",
        )?;
        gesture(
            &mut f,
            &[
                ("p", 150),
                ("m -40 20", 150),
                ("m -280 120", 250),
                ("r", 250),
            ],
        )?;
        let value = state(&f, &qml)?;
        save(&f.options.output.join("drag-state.json"), &value)?;
        wait_true("Freeform desktop position", || {
            Ok(has_position(&state(&f, &qml)?))
        })?;
        dragged = at(&state(&f, &qml)?["items"], 1)?.clone();
        require(
            number(&dragged["x"])? < number(&before["x"])? - 200.0
                && number(&dragged["y"])? > number(&before["y"])? + 70.0,
            "Freeform drag did not move expected item",
        )?;
        require(
            fs::read_to_string(&one)? == "Disposable desktop fixture",
            "Freeform drag changed file",
        )?;
        f.screenshot("dragged.png", false)?;
        if f.options.multi_drag {
            ipc(&f, &qml, "selectAll", &[])?;
            pause(700);
            let old = state(&f, &qml)?;
            let rect = &old["geometry"];
            let target = at(&old["items"], 1)?;
            f.cursor(
                (number(&rect["x"])? + number(&target["x"])? + 50.0) as i64,
                (number(&rect["y"])? + number(&target["y"])? + 40.0) as i64,
            )?;
            gesture(
                &mut f,
                &[
                    ("m 1 1", 150),
                    ("p", 150),
                    ("m -30 20", 150),
                    ("m -150 100", 200),
                    ("r", 0),
                ],
            )?;
            wait_true("Multi-item positions", || {
                Ok(state(&f, &qml)?["layout"]["positions"]
                    .as_object()
                    .is_some_and(|map| map.len() == 2))
            })?;
            let moved = state(&f, &qml)?;
            let a = at(&moved["items"], 0)?;
            let a_old = at(&old["items"], 0)?;
            let b = at(&moved["items"], 1)?;
            let b_old = at(&old["items"], 1)?;
            let dx = number(&a["x"])? - number(&a_old["x"])?;
            let dy = number(&a["y"])? - number(&a_old["y"])?;
            require(dx < -50.0 && dy > 50.0, "Multi-drag offset")?;
            require(
                ((number(&b["x"])? - number(&b_old["x"])?) - dx).abs() < 1.0
                    && ((number(&b["y"])? - number(&b_old["y"])?) - dy).abs() < 1.0,
                "Multi-drag did not preserve offsets",
            )?;
            require(
                folder.is_dir() && fs::read_to_string(&one)? == "Disposable desktop fixture",
                "Multi-drag changed originals",
            )?;
            dragged = b.clone();
            f.screenshot("multi-dragged.png", false)?;
        }
        f.cursor(400, 500)?;
        gesture(&mut f, &[("m 1 1", 150), ("p", 150), ("r", 150)])?;
        wait_true("Desktop click-through", || {
            Ok(state(&f, &qml)?["backgroundClicks"] == 1)
        })?;
        ipc(&f, &qml, "select", &["1"])?;
    }
    f.key("Delete")?;
    wait_true("Trash confirmation", || {
        Ok(state(&f, &qml)?["confirm"] == true)
    })?;
    require(one.exists(), "Trash ran before confirmation")?;
    f.screenshot("confirmation.png", false)?;
    f.key("Escape")?;
    wait_true("Trash Escape", || Ok(state(&f, &qml)?["confirm"] == false))?;
    require(one.exists(), "Escape trashed file")?;
    ipc(&f, &qml, "rename", &["renamed.txt"])?;
    let renamed = f.desktop.join("renamed.txt");
    wait_true("Desktop rename", || Ok(renamed.exists() && !one.exists()))?;
    if f.options.layout {
        wait_true("Renamed layout row", || {
            Ok(at(&state(&f, &qml)?["items"], 1)?["row"]["name"] == "renamed.txt")
        })?;
        let value = state(&f, &qml)?;
        let item = at(&value["items"], 1)?;
        require(
            item["key"] == dragged["key"]
                && (number(&item["x"])? - number(&dragged["x"])?).abs() < 1.0,
            "Rename changed dragged identity",
        )?;
        ipc(&f, &qml, "group", &["kind"])?;
        wait_true("Kind stack", || Ok(stack(&state(&f, &qml)?).is_ok()))?;
        let value = state(&f, &qml)?;
        let rect = &value["geometry"];
        let group = stack(&value)?;
        f.cursor(
            (number(&rect["x"])? + number(&group["x"])? + 50.0) as i64,
            (number(&rect["y"])? + number(&group["y"])? + 40.0) as i64,
        )?;
        gesture(&mut f, &[("m 1 1", 150), ("p", 150), ("r", 150)])?;
        wait_true("Stack expansion", || {
            Ok(rows(&state(&f, &qml)?["items"])?.len() == 3)
        })?;
        f.screenshot("expanded-stack.png", false)?;
        if f.options.multi_drag {
            pause(700);
            let value = state(&f, &qml)?;
            let old_stack = stack(&value)?.clone();
            let rect = &value["geometry"];
            f.cursor(
                (number(&rect["x"])? + number(&old_stack["x"])? + 50.0) as i64,
                (number(&rect["y"])? + number(&old_stack["y"])? + 40.0) as i64,
            )?;
            gesture(
                &mut f,
                &[
                    ("m 1 1", 150),
                    ("p", 150),
                    ("m -30 20", 150),
                    ("m -200 120", 200),
                    ("r", 0),
                ],
            )?;
            wait_true("Stack position", || {
                Ok(state(&f, &qml)?["layout"]["stacks"]
                    .as_object()
                    .is_some_and(|map| !map.is_empty()))
            })?;
            let value = state(&f, &qml)?;
            let positioned = value["layout"]["stacks"].clone();
            let new_stack = stack(&value)?;
            require(
                number(&new_stack["x"])? < number(&old_stack["x"])? - 100.0
                    && number(&new_stack["y"])? > number(&old_stack["y"])? + 50.0,
                "Stack did not move",
            )?;
            let child = rows(&value["items"])?
                .iter()
                .find(|item| item["parentStack"] == new_stack["key"])
                .ok_or("Missing stack child")?;
            require(
                (number(&child["x"])? - number(&new_stack["x"])?).abs() < 1.0
                    && number(&child["y"])? > number(&new_stack["y"])?,
                "Stack child offset",
            )?;
            ipc(&f, &qml, "group", &["none"])?;
            wait_true("Group none", || {
                Ok(state(&f, &qml)?["layout"]["grouping"] == "none")
            })?;
            ipc(&f, &qml, "group", &["kind"])?;
            wait_true("Group kind", || {
                Ok(state(&f, &qml)?["layout"]["grouping"] == "kind")
            })?;
            require(
                state(&f, &qml)?["layout"]["stacks"] == positioned,
                "Stack position lost across grouping",
            )?;
            f.screenshot("stack-dragged.png", false)?;
        }
        ipc(&f, &qml, "group", &["none"])?;
        wait_true("Group cleared", || {
            Ok(state(&f, &qml)?["layout"]["grouping"] == "none")
        })?;
    }
    let new = f.desktop.join("new.txt");
    fs::write(&new, "watcher fixture")?;
    wait_true("File watcher add", || Ok(state(&f, &qml)?["count"] == 3))?;
    fs::remove_file(&new)?;
    wait_true("File watcher remove", || Ok(state(&f, &qml)?["count"] == 2))?;
    ipc(&f, &qml, "select", &["1"])?;
    ipc(&f, &qml, "askTrash", &[])?;
    require(renamed.exists(), "Trash dispatched before confirmation")?;
    ipc(&f, &qml, "confirmTrash", &[])?;
    wait_true("Confirmed Trash", || Ok(!renamed.exists()))?;
    require(folder.is_dir(), "Unselected Desktop folder changed")?;
    let mut checks = vec![
        "configured Desktop directory",
        "real bottom-layer click focus",
        "physical Delete and Escape confirmation",
        "rename",
        "live add/remove events",
        "explicit confirmed Trash",
        "unselected item preserved",
    ];
    if f.options.layout {
        checks.extend([
            "real pointer freeform drag persisted",
            "rename retained dragged identity",
            "empty area click-through",
            "kind stack real pointer expansion",
        ]);
    }
    if f.options.multi_drag {
        checks.extend([
            "real multi-item drag preserves offsets",
            "whole expanded stack pointer drag",
            "stack position survives grouping switch",
        ]);
    }
    save(
        &f.options.output.join("result.json"),
        &json!({"passed":true,"checks":checks}),
    )?;
    Ok(())
}
