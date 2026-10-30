//! Disposable real lock surfaces and pointer recipients; never handles credentials.
use serde_json::json;
use std::{
    error::Error,
    fs::File,
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    outputs: Vec<wl_output::WlOutput>,
    surfaces: Vec<wl_surface::WlSurface>,
    buffers: Vec<wl_buffer::WlBuffer>,
    files: Vec<File>,
    pointer: Option<wl_pointer::WlPointer>,
    configured: usize,
    locked: bool,
    failed: Option<String>,
}
impl State {
    fn paint(
        &mut self,
        index: usize,
        width: u32,
        height: u32,
        qh: &QueueHandle<Self>,
    ) -> Result<()> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 || self.buffers.len() >= 16 {
            return Err("Lock surface bound".into());
        }
        let size = u64::from(width) * u64::from(height) * 4;
        let fd =
            unsafe { libc::memfd_create(c"lumen-disposable-lock".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.set_len(size)?;
        let chunk = [16u8, 20, 28, 255].repeat(4096);
        let mut left = size;
        while left > 0 {
            let count = usize::try_from(left.min(chunk.len() as u64))?;
            file.write_all(&chunk[..count])?;
            left -= count as u64;
        }
        let pool =
            self.shm
                .as_ref()
                .ok_or("shm")?
                .create_pool(file.as_fd(), i32::try_from(size)?, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );
        pool.destroy();
        let surface = self.surfaces.get(index).ok_or("Lock surface index")?;
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();
        self.buffers.push(buffer);
        self.files.push(file);
        self.configured += 1;
        println!(
            "{}",
            json!({"configured":index,"width":width,"height":height})
        );
        std::io::stdout().flush()?;
        Ok(())
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "ext_session_lock_manager_v1" => {
                    state.manager = Some(registry.bind(name, 1, qh, ()))
                }
                "wl_output" => {
                    if state.outputs.len() >= 4 {
                        state.failed = Some("Output bound".into());
                    } else {
                        state
                            .outputs
                            .push(registry.bind(name, version.min(4), qh, ()));
                    }
                }
                "wl_seat" => {
                    let _: wl_seat::WlSeat = registry.bind(name, version.min(8), qh, ());
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button {
            serial,
            time,
            button,
            state,
        } = event
        {
            println!(
                "{}",
                json!({"pointer":true,"serial":serial,"time":time,"button":button,"pressed":matches!(state,WEnum::Value(wl_pointer::ButtonState::Pressed))})
            );
            let _ = std::io::stdout().flush();
        }
    }
}
impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => {
                state.locked = true;
                println!("{}", json!({"locked":true}));
                let _ = std::io::stdout().flush();
            }
            ext_session_lock_v1::Event::Finished => {
                state.failed = Some("Lock request unavailable".into())
            }
            _ => {}
        }
    }
}
impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, usize> for State {
    fn event(
        state: &mut Self,
        surface: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        index: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            surface.ack_configure(serial);
            if let Err(error) = state.paint(*index, width, height, qh) {
                state.failed = Some(error.to_string());
            }
        }
    }
}
delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
fn run() -> Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<State>();
    let qh = queue.handle();
    let mut state = State::default();
    let _registry = connection.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    if state.outputs.is_empty() || state.outputs.len() > 4 {
        return Err("Connected output required".into());
    }
    let lock = state
        .manager
        .as_ref()
        .ok_or("Lock manager missing")?
        .lock(&qh, ());
    let mut roles = Vec::new();
    for (index, output) in state.outputs.clone().iter().enumerate() {
        let surface = state
            .compositor
            .as_ref()
            .ok_or("Compositor missing")?
            .create_surface(&qh, ());
        roles.push(lock.get_lock_surface(&surface, output, &qh, index));
        state.surfaces.push(surface);
    }
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    if let Some(error) = state.failed.take() {
        return Err(error.into());
    }
    if !state.locked || state.configured < state.outputs.len() {
        return Err("Lock surfaces not configured".into());
    }
    println!("{}", json!({"ready":true,"surfaces":state.outputs.len()}));
    std::io::stdout().flush()?;
    let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut input = Vec::new();
    loop {
        if Instant::now() > deadline {
            return Err("Lock client lifetime bound".into());
        }
        queue.dispatch_pending(&mut state)?;
        if let Some(error) = state.failed.take() {
            return Err(error.into());
        }
        connection.flush()?;
        let Some(guard) = connection.prepare_read() else {
            continue;
        };
        let mut descriptors = [
            libc::pollfd {
                fd: connection.as_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let polled = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, 100) };
        if polled < 0 {
            drop(guard);
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(std::io::Error::last_os_error().into());
        }
        if descriptors[0].revents & libc::POLLIN != 0 {
            guard.read()?;
        } else {
            drop(guard);
        }
        if descriptors[1].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            let mut bytes = [0; 16];
            match std::io::stdin().read(&mut bytes) {
                Ok(0) => return Err("Lock owner input closed".into()),
                Ok(count) => input.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
            if input.len() > 16 {
                return Err("Lock command bound".into());
            }
            if input.contains(&b'\n') {
                if input != b"u\n" {
                    return Err("Unknown lock command".into());
                }
                lock.unlock_and_destroy();
                queue.roundtrip(&mut state)?;
                return Ok(());
            }
        }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Projection lock client: {error}");
        std::process::exit(1);
    }
}
