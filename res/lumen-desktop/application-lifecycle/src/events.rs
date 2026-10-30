//! Finite reads of already queued event frames, including partial-header refusal.
use crate::{Result, receive};
use serde_json::Value;
use std::{
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::{Duration, Instant},
};
pub const MAX_FRAMES: usize = 32;
pub const MAX_BYTES: usize = 256 * 1024;
/// Workspace/output observations refresh placement without manufacturing an
/// application-focus event or changing activation history.
pub fn window_event(kind:u32)->Result<bool>{
    match kind {0x8000_0003=>Ok(true),0x8000_0000|0x8000_0001=>Ok(false),_=>Err("Unexpected compositor event".into())}
}
#[cfg(test)] mod scope_event_tests {
    #[test] fn placement_events_are_not_focus_events(){
        assert_eq!(super::window_event(0x8000_0003),Ok(true));
        for kind in [0x8000_0000,0x8000_0001]{assert_eq!(super::window_event(kind),Ok(false));}
        assert!(super::window_event(0x8000_0002).is_err());
    }
}
fn transfer(stream: &UnixStream, bytes: &mut [u8], write: bool, deadline: Instant) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Compositor exchange deadline expired".into());
        }
        let mut fd = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: if write { libc::POLLOUT } else { libc::POLLIN },
            revents: 0,
        };
        let result = unsafe {
            libc::poll(
                &mut fd,
                1,
                remaining
                    .as_millis()
                    .saturating_add(1)
                    .min(i32::MAX as u128) as i32,
            )
        };
        if result == 0 {
            return Err("Compositor exchange deadline expired".into());
        }
        if result < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.to_string());
        }
        if Instant::now() >= deadline {
            return Err("Compositor exchange deadline expired".into());
        }
        let part = &mut bytes[offset..];
        let count = unsafe {
            if write {
                libc::send(
                    stream.as_raw_fd(),
                    part.as_ptr().cast(),
                    part.len(),
                    libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
                )
            } else {
                libc::recv(
                    stream.as_raw_fd(),
                    part.as_mut_ptr().cast(),
                    part.len(),
                    libc::MSG_DONTWAIT,
                )
            }
        };
        if count < 0 {
            let e = io::Error::last_os_error();
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                continue;
            }
            return Err(e.to_string());
        }
        if count == 0 {
            return Err("Compositor exchange closed".into());
        }
        offset += count as usize;
    }
    Ok(())
}
/// GET_TREE uses a single absolute deadline across all partial writes/reads.
pub fn tree(stream: &UnixStream, deadline: Instant) -> Result<Value> {
    let mut request = b"i3-ipc".to_vec();
    request.extend_from_slice(&0u32.to_le_bytes());
    request.extend_from_slice(&4u32.to_le_bytes());
    transfer(stream, &mut request, true, deadline)?;
    let mut header = [0; 14];
    transfer(stream, &mut header, false, deadline)?;
    if &header[..6] != b"i3-ipc" || u32::from_le_bytes(header[10..14].try_into().unwrap()) != 4 {
        return Err("Unexpected compositor tree reply".into());
    }
    let size = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if size > crate::MAX_FRAME {
        return Err("Compositor response exceeds limit".into());
    }
    let mut payload = vec![0; size];
    transfer(stream, &mut payload, false, deadline)?;
    serde_json::from_slice(&payload).map_err(|_| "Invalid compositor tree".into())
}
pub fn drain(stream: &mut UnixStream) -> Result<Vec<(u32, Value)>> {
    let deadline = Instant::now() + Duration::from_millis(50);
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    loop {
        if Instant::now() >= deadline {
            return Err("Compositor event drain deadline expired".into());
        }
        let mut header = [0u8; 14];
        let count = unsafe {
            libc::recv(
                stream.as_raw_fd(),
                header.as_mut_ptr().cast(),
                header.len(),
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(rows);
            }
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.to_string());
        }
        if count == 0 {
            return Err("Compositor event stream closed".into());
        }
        if rows.len() >= MAX_FRAMES || Instant::now() >= deadline {
            return Err("Compositor event drain budget exhausted".into());
        }
        if count == 14 {
            if &header[..6] != b"i3-ipc" {
                return Err("Invalid queued event header".into());
            }
            let length = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
            let frame = length
                .checked_add(14)
                .ok_or("Invalid queued event length")?;
            if frame > MAX_BYTES.saturating_sub(bytes) {
                return Err("Compositor event byte budget exhausted".into());
            }
            let mut available: libc::c_int = 0;
            if unsafe { libc::ioctl(stream.as_raw_fd(), libc::FIONREAD, &mut available) } < 0 {
                return Err(io::Error::last_os_error().to_string());
            }
            if available >= 0 && available as usize >= frame {
                // There is exactly one reader. The entire bounded frame is in
                // the socket before receive allocates or reads its payload.
                rows.push(receive(stream)?);
                bytes += frame;
                continue;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
