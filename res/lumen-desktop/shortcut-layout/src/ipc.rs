//! A single read-only IPC request with an absolute transport deadline and a
//! declared frame cap checked before allocation. No compositor command channel.
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::{ffi::OsStrExt, net::UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};
const MAX: usize = 2 * 1024 * 1024;
fn ready(socket: &UnixStream, events: i16, end: Instant) -> Result<(), String> {
    loop {
        let left = end
            .checked_duration_since(Instant::now())
            .ok_or("Native keyboard transport deadline exceeded")?;
        let mut fd = libc::pollfd {
            fd: socket.as_raw_fd(),
            events,
            revents: 0,
        };
        let rc = unsafe {
            libc::poll(
                &mut fd,
                1,
                left.as_millis().min(i32::MAX as u128).max(1) as i32,
            )
        };
        if rc < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if rc <= 0 {
            return Err("Native keyboard transport unavailable or timed out".into());
        }
        if fd.revents & events != 0 {
            return Ok(());
        }
        return Err("Native keyboard transport disconnected".into());
    }
}
pub fn connect(path: &Path, end: Instant) -> Result<UnixStream, String> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.len() >= 108 || bytes.contains(&0) {
        return Err("Invalid native keyboard socket path".into());
    }
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd < 0 {
        return Err("Native keyboard socket unavailable".into());
    }
    let socket = unsafe { UnixStream::from_raw_fd(fd) };
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as u16;
    for (to, from) in address.sun_path.iter_mut().zip(bytes) {
        *to = *from as i8;
    }
    let rc = unsafe {
        libc::connect(
            fd,
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    };
    if rc != 0 {
        let error = std::io::Error::last_os_error().raw_os_error();
        if error != Some(libc::EINPROGRESS) && error != Some(libc::EAGAIN) {
            return Err("Native keyboard connection failed".into());
        }
        ready(&socket, libc::POLLOUT, end)?;
        let mut error = 0i32;
        let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut i32).cast(),
                &mut size,
            )
        } != 0
            || error != 0
        {
            return Err("Native keyboard connection failed".into());
        }
    }
    Ok(socket)
}
fn read_exact(socket: &mut UnixStream, mut bytes: &mut [u8], end: Instant) -> Result<(), String> {
    while !bytes.is_empty() {
        ready(socket, libc::POLLIN, end)?;
        match socket.read(bytes) {
            Ok(0) => return Err("Native keyboard reply closed".into()),
            Ok(count) => {
                bytes = &mut bytes[count..];
            }
            Err(error)
                if [
                    std::io::ErrorKind::WouldBlock,
                    std::io::ErrorKind::Interrupted,
                ]
                .contains(&error.kind()) => {}
            Err(_) => return Err("Native keyboard reply failed".into()),
        }
    }
    Ok(())
}
pub fn seats(socket: &mut UnixStream, end: Instant) -> Result<Vec<u8>, String> {
    let mut request = b"i3-ipc".to_vec();
    request.extend(0u32.to_le_bytes());
    request.extend(101u32.to_le_bytes());
    let mut bytes = request.as_slice();
    while !bytes.is_empty() {
        ready(socket, libc::POLLOUT, end)?;
        match socket.write(bytes) {
            Ok(0) => return Err("Native keyboard request closed".into()),
            Ok(n) => bytes = &bytes[n..],
            Err(e)
                if [
                    std::io::ErrorKind::WouldBlock,
                    std::io::ErrorKind::Interrupted,
                ]
                .contains(&e.kind()) => {}
            Err(_) => return Err("Native keyboard request failed".into()),
        }
    }
    let mut header = [0; 14];
    read_exact(socket, &mut header, end)?;
    let size = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
    if &header[..6] != b"i3-ipc"
        || u32::from_le_bytes(header[10..14].try_into().unwrap()) != 101
        || size > MAX
    {
        return Err("Native keyboard frame exceeds limits".into());
    }
    let mut bytes = vec![0; size];
    read_exact(socket, &mut bytes, end)?;
    Ok(bytes)
}
pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(1)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_partial_frame_times_out_without_waiting_for_writer_close() {
        let (mut client, mut peer) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let mut request = [0; 14];
            peer.read_exact(&mut request).unwrap();
            assert_eq!(u32::from_le_bytes(request[10..].try_into().unwrap()), 101);
            peer.write_all(b"i3").unwrap();
            std::thread::sleep(Duration::from_millis(180));
        });
        let start = Instant::now();
        assert!(
            seats(&mut client, start + Duration::from_millis(40))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        thread.join().unwrap();
    }
    #[test]
    fn actual_oversize_header_refuses_before_payload_allocation() {
        let (mut client, mut peer) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        let thread = std::thread::spawn(move || {
            let mut request = [0; 14];
            peer.read_exact(&mut request).unwrap();
            let mut response = b"i3-ipc".to_vec();
            response.extend(((MAX + 1) as u32).to_le_bytes());
            response.extend(101u32.to_le_bytes());
            peer.write_all(&response).unwrap();
        });
        assert!(
            seats(&mut client, deadline())
                .unwrap_err()
                .contains("limits")
        );
        thread.join().unwrap();
    }
}
