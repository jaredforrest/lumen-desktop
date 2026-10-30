use lumen_application_lifecycle::{
    events::{MAX_BYTES, MAX_FRAMES, drain},
    send,
};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
#[test]
fn event_frames_are_bounded_before_reading_or_allocating_payloads() {
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    assert!(drain(&mut reader).unwrap().is_empty());
    send(&mut writer, 0x8000_0003, "{\"change\":\"title\"}").unwrap();
    send(&mut writer, 0x8000_0003, "{\"change\":\"new\"}").unwrap();
    let values = drain(&mut reader).unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].1["change"], "title");
    assert_eq!(values[1].1["change"], "new");
    writer.write_all(b"i3-").unwrap();
    let start = Instant::now();
    assert!(drain(&mut reader).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    let mut header = b"i3-ipc".to_vec();
    header.extend_from_slice(&(MAX_BYTES as u32).to_le_bytes());
    header.extend_from_slice(&0x8000_0003u32.to_le_bytes());
    writer.write_all(&header).unwrap();
    assert!(drain(&mut reader).is_err());
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    for _ in 0..=MAX_FRAMES {
        send(&mut writer, 0x8000_0003, "{}").unwrap();
    }
    assert!(drain(&mut reader).is_err());
}
#[test]
fn partial_payload_and_malformed_header_retire_with_finite_wait() {
    for bytes in [b"wrong-header!!".to_vec(), {
        let mut h = b"i3-ipc".to_vec();
        h.extend_from_slice(&40u32.to_le_bytes());
        h.extend_from_slice(&0x8000_0003u32.to_le_bytes());
        h.extend_from_slice(b"{");
        h
    }] {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(&bytes).unwrap();
        let start = Instant::now();
        assert!(drain(&mut reader).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
#[test]
fn tree_exchange_has_one_deadline_even_with_a_trickling_peer() {
    use std::io::Read;
    let (reader, mut writer) = UnixStream::pair().unwrap();
    let peer = std::thread::spawn(move || {
        let mut request = [0; 14];
        writer.read_exact(&mut request).unwrap();
        for byte in b"i3-ipc" {
            if writer.write_all(&[*byte]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let start = Instant::now();
    assert!(
        lumen_application_lifecycle::events::tree(&reader, start + Duration::from_millis(50))
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_millis(250));
    drop(reader);
    peer.join().unwrap();
}
