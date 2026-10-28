//! Shared retained-browser fixture mechanics. The browser itself is always supplied explicitly.
pub mod accessibility;
pub mod accessibility_dbus;
pub mod boundary;
pub mod capabilities;
pub mod keyboard;
pub mod native;
pub mod paint;
pub mod portal;
pub mod presentation;
pub mod profile;
pub mod protocol;
pub mod server;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    message.into().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::Digest;
    use std::collections::BTreeMap;
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::os::unix::net::UnixStream;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn provider_startup_restores_client_policy() {
        let mut environment = BTreeMap::from([
            ("FONTCONFIG_FILE".into(), "client-font".into()),
            ("XDG_CURRENT_DESKTOP".into(), "sway".into()),
            ("GTK_USE_PORTAL".into(), "1".into()),
            ("DBUS_SESSION_BUS_ADDRESS".into(), "old-private-bus".into()),
        ]);
        let child = presentation::start_preserving_client(&mut environment, |env| {
            env.insert("FONTCONFIG_FILE".into(), "provider-font".into());
            env.insert("XDG_CURRENT_DESKTOP".into(), "GNOME".into());
            env.insert("GTK_USE_PORTAL".into(), "0".into());
            env.insert("DBUS_SESSION_BUS_ADDRESS".into(), "new-private-bus".into());
            Ok::<_, String>(
                ["FONTCONFIG_FILE", "XDG_CURRENT_DESKTOP", "GTK_USE_PORTAL"]
                    .map(|key| env[key].clone()),
            )
        })
        .unwrap();
        assert_eq!(child, ["provider-font", "GNOME", "0"]);
        assert_eq!(environment["FONTCONFIG_FILE"], "client-font");
        assert_eq!(environment["XDG_CURRENT_DESKTOP"], "sway");
        assert_eq!(environment["GTK_USE_PORTAL"], "1");
        assert_eq!(environment["DBUS_SESSION_BUS_ADDRESS"], "new-private-bus");
    }
    #[test]
    fn provider_failure_restores_absent_policy() {
        let mut environment = BTreeMap::new();
        let result: std::result::Result<(), &str> =
            presentation::start_preserving_client(&mut environment, |env| {
                for key in presentation::KEYS {
                    env.insert(key.into(), "provider".into());
                }
                Err("Synthetic provider failure")
            });
        assert_eq!(result, Err("Synthetic provider failure"));
        assert!(environment.is_empty());
    }
    #[test]
    fn internal_entry_rejects_ambient_execution_before_state() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("artifacts");
        let error = boundary::validate_inside(&output).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Private application boundary required")
        );
        assert!(fs::read_dir(root.path()).unwrap().next().is_none());
    }
    #[test]
    fn invocation_rejects_host_profile_and_external_origin() {
        let tool = std::env::var("PATH").ok().and_then(|path| {
            path.split(':')
                .map(std::path::Path::new)
                .map(|dir| dir.join("lumen-browser-invocation"))
                .find(|path| path.is_file())
        });
        let Some(tool) = tool else { return };
        let invoke = |profile: &str, url: &str| {
            Command::new(&tool)
                .args([
                    "/nix/store/fixture-browser/bin/brave",
                    profile,
                    url,
                    "normal",
                    "false",
                ])
                .output()
                .unwrap()
        };
        let actual = invoke(
            "/home/fixture/Literal # profile",
            "http://127.0.0.1:8080/page",
        );
        assert!(actual.status.success());
        let command = String::from_utf8(actual.stdout).unwrap();
        assert!(command.contains("--user-data-dir=/home/fixture/Literal # profile"));
        assert!(
            ![
                "--no-sandbox",
                "--disable-setuid-sandbox",
                "--disable-seccomp",
                "--use-fake-ui"
            ]
            .iter()
            .any(|flag| command.contains(flag))
        );
        for (profile, url) in [
            ("/home/other/profile", "http://127.0.0.1:8080/page"),
            ("/home/fixture/../other", "http://127.0.0.1:8080/page"),
            ("/home/fixture/profile", "https://example.invalid/"),
        ] {
            assert!(!invoke(profile, url).status.success());
        }
    }
    #[test]
    fn stable_physical_map_covers_printable_us_input() {
        for character in (32_u8..127).map(char::from) {
            let events = keyboard::text_events(&character.to_string()).unwrap();
            assert_eq!(events.last(), Some(&("m", 0, 0)));
            assert_eq!(events[1].0, "k");
            assert_eq!(events[1].1, events[2].1);
            assert_eq!(events[1].2, 1);
            assert_eq!(events[2].2, 0);
        }
        assert_eq!(keyboard::special("Escape"), Some(1));
        assert!(keyboard::text_events("é").is_err());
    }
    #[test]
    fn painted_readback_requires_ink_and_alignment() {
        use paint::{Image, match_paint};
        let mut reference = Image::solid(64, 32, [255; 3]);
        for y in 8..=20 {
            for x in 8..=30 {
                reference.set(x, y, [0x21, 0x38, 0x4a]);
            }
        }
        let mut screen = Image::solid(100, 80, [255; 3]);
        assert_eq!(
            match_paint(&reference, &screen, (10.0, 12.0))["painted"],
            false
        );
        assert_eq!(
            match_paint(&Image::solid(64, 32, [255; 3]), &screen, (10.0, 12.0))["painted"],
            false
        );
        for y in 0..32 {
            for x in 0..64 {
                screen.set(10 + x, 12 + y, reference.pixel(x, y));
            }
        }
        assert_eq!(
            match_paint(&reference, &screen, (10.0, 12.0))["painted"],
            true
        );
        for shift in [1, 2] {
            let mut screen = Image::solid(100, 80, [255; 3]);
            for y in 0..32 {
                for x in 0..64 {
                    screen.set(10 + x + shift, 12 + y, reference.pixel(x, y));
                }
            }
            assert_eq!(
                match_paint(&reference, &screen, (10.0, 12.0))["painted"],
                shift == 1
            );
        }
    }
    #[test]
    fn one_action_does_not_close_capability() {
        let mut inventory =
            capabilities::Inventory::new(json!({"product":"synthetic version"})).unwrap();
        inventory
            .record(
                "APP-BROWSE-01",
                "new_tab",
                "verified_compatible",
                json!({"capture":"synthetic"}),
                "",
            )
            .unwrap();
        let row = &inventory.snapshot()[0];
        assert_eq!(row["status"], "unassessed");
        assert_eq!(row["supported_actions"], json!(["new_tab"]));
        assert!(
            row["remaining_actions"]
                .as_array()
                .unwrap()
                .contains(&json!("close_tab"))
        );
        assert!(
            inventory
                .record(
                    "APP-BROWSE-01",
                    "new_tab",
                    "verified_compatible",
                    json!({"capture":"replacement"}),
                    ""
                )
                .is_err()
        );
    }
    #[test]
    fn download_observer_is_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("Literal # é");
        let default = profile.join("Default");
        fs::create_dir_all(&default).unwrap();
        let path = default.join("History");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE downloads (id INTEGER,guid TEXT,current_path TEXT,target_path TEXT,received_bytes INTEGER,total_bytes INTEGER,state INTEGER,interrupt_reason INTEGER);").unwrap();
            conn.execute(
                "INSERT INTO downloads VALUES (1,?1,?2,?3,12,24,0,0)",
                rusqlite::params![
                    "synthetic-guid",
                    b"/synthetic/partial",
                    b"/synthetic/complete"
                ],
            )
            .unwrap();
        }
        let before = fs::read(&path).unwrap();
        let rows = profile::download_rows(&profile).unwrap();
        assert_eq!(rows[0]["state"], 0);
        assert_eq!(rows[0]["target_path"], "/synthetic/complete");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    fn get(origin: &str, path: &str) -> Vec<u8> {
        let address = origin.strip_prefix("http://").unwrap();
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let i = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        bytes[i..].to_vec()
    }
    #[test]
    fn gated_loopback_download_has_exact_bytes() {
        let mut server = server::FixtureServer::start().unwrap();
        assert!(
            get(server.origin(), "/")
                .windows(21)
                .any(|v| v == b"Disposable local page")
        );
        let origin = server.origin().to_string();
        let worker = thread::spawn(move || get(&origin, "/download/fixed"));
        let end = std::time::Instant::now() + Duration::from_secs(2);
        while server.snapshot().first().is_none_or(|v| v["sent"] != 65536)
            && std::time::Instant::now() < end
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(server.snapshot()[0]["complete"], false);
        server.gate("fixed").unwrap().open();
        let bytes = worker.join().unwrap();
        assert_eq!(bytes.len(), server::payload().len());
        assert_eq!(
            format!("{:x}", sha2::Sha256::digest(&bytes)),
            server::digest()
        );
        server.close();
    }
    #[test]
    fn fragmented_frames_and_events() {
        let (client_writer, mut peer_reader) = UnixStream::pair().unwrap();
        let (mut peer_writer, client_reader) = UnixStream::pair().unwrap();
        let read = unsafe { File::from_raw_fd(client_reader.into_raw_fd()) };
        let write = unsafe { File::from_raw_fd(client_writer.into_raw_fd()) };
        let worker = thread::spawn(move || {
            let mut bytes = Vec::new();
            loop {
                let mut one = [0];
                peer_reader.read_exact(&mut one).unwrap();
                if one[0] == 0 {
                    break;
                }
                bytes.push(one[0]);
            }
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let event = serde_json::to_vec(
                &json!({"method":"Fixture.observed","params":{"method":request["method"]}}),
            )
            .unwrap();
            let response = serde_json::to_vec(
                &json!({"id":request["id"],"result":{"observed":request["params"]}}),
            )
            .unwrap();
            for bytes in [event, response] {
                for chunk in bytes.chunks(3) {
                    peer_writer.write_all(chunk).unwrap();
                }
                peer_writer.write_all(&[0]).unwrap();
            }
        });
        let mut frames = protocol::Frames::new(read, write);
        let result = frames
            .request(
                "Fixture.inspect",
                json!({"value":"literal é"}),
                None,
                Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(result, json!({"observed":{"value":"literal é"}}));
        assert_eq!(frames.events[0]["params"]["method"], "Fixture.inspect");
        worker.join().unwrap();
    }
}
