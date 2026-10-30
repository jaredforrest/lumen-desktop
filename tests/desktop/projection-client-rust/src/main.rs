//! Disposable actual XDG fullscreen/output/dialog requests, never a host client.
use serde_json::json;
use std::{
    error::Error,
    fs::File,
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::xdg::{
    dialog::v1::client::{xdg_dialog_v1, xdg_wm_dialog_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Row {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    top: xdg_toplevel::XdgToplevel,
}
#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm: Option<xdg_wm_base::XdgWmBase>,
    dialogs: Option<xdg_wm_dialog_v1::XdgWmDialogV1>,
    outputs: Vec<(u32, wl_output::WlOutput, Option<String>)>,
    rows: Vec<Row>,
    dialog: Option<xdg_dialog_v1::XdgDialogV1>,
    buffer: Option<wl_buffer::WlBuffer>,
    fullscreen: bool,
    configures: u32,
    error: Option<String>,
}
impl State {
    fn window(
        &mut self,
        qh: &QueueHandle<Self>,
        id: usize,
        fixed: bool,
        modal: bool,
        app: &str,
    ) -> Result<()> {
        let surface = self
            .compositor
            .as_ref()
            .ok_or("compositor")?
            .create_surface(qh, ());
        let xdg = self
            .wm
            .as_ref()
            .ok_or("xdg shell")?
            .get_xdg_surface(&surface, qh, id);
        let top = xdg.get_toplevel(qh, id);
        top.set_app_id(app.into());
        top.set_title(format!("Projection protocol {id}"));
        if fixed {
            top.set_min_size(400, 300);
            top.set_max_size(400, 300);
        }
        if id > 0 {
            top.set_parent(Some(&self.rows[id - 1].top));
            let dialog = self
                .dialogs
                .as_ref()
                .ok_or("dialog protocol")?
                .get_xdg_dialog(&top, qh, ());
            if modal {
                dialog.set_modal();
            }
            self.dialog = Some(dialog);
        }
        surface.commit();
        self.rows.push(Row { surface, xdg, top });
        Ok(())
    }
    fn command(&mut self, qh: &QueueHandle<Self>, command: &str) -> Result<bool> {
        match command {
            "fullscreen none" => self.rows[0].top.set_fullscreen(None),
            "fullscreen HEADLESS-1" | "fullscreen HEADLESS-2" => {
                let name = command
                    .strip_prefix("fullscreen ")
                    .ok_or("output request")?;
                let output = self
                    .outputs
                    .iter()
                    .find(|(_, _, n)| n.as_deref() == Some(name))
                    .ok_or("Requested actual output unavailable")?;
                self.rows[0].top.set_fullscreen(Some(&output.1));
            }
            "unfullscreen" => self.rows[0].top.unset_fullscreen(),
            "modal" | "owned" | "owned-utility" => {
                if self.rows.len() != 1 {
                    return Err("Dialog already exists".into());
                }
                self.window(
                    qh,
                    1,
                    false,
                    command == "modal",
                    if command == "owned-utility" {
                        "org.example.ProjectionUtility"
                    } else {
                        "org.example.ProjectionProtocol"
                    },
                )?;
            }
            "extend" => {
                if self.rows.len() >= 16 {
                    return Err("Ordinary chain extension is bounded at16 views".into());
                }
                self.window(
                    qh,
                    self.rows.len(),
                    false,
                    false,
                    "org.example.ProjectionProtocol",
                )?;
            }
            "extend17" => {
                if self.rows.len() != 16 {
                    return Err("Overflow witness requires exact16 views".into());
                }
                self.window(qh, 16, false, false, "org.example.ProjectionProtocol")?;
            }
            "unparent" => {
                if self.rows.len() != 2 {
                    return Err("Unparent requires one child".into());
                }
                self.rows[1].top.set_parent(None);
            }
            "unmodal" => {
                if self.rows.len() != 2 {
                    return Err("No owned dialog".into());
                }
                if let Some(dialog) = self.dialog.take() {
                    dialog.destroy();
                }
                let row = self.rows.pop().ok_or("Dialog row")?;
                row.top.destroy();
                row.xdg.destroy();
                row.surface.destroy();
            }
            "quit" => return Ok(false),
            _ => return Err("Unknown bounded protocol command".into()),
        }
        Ok(true)
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
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => state.wm = Some(registry.bind(name, version.min(3), qh, ())),
                "xdg_wm_dialog_v1" => state.dialogs = Some(registry.bind(name, 1, qh, ())),
                "wl_output" => {
                    if state.outputs.len() >= 8 || version < 4 {
                        state.error = Some("Output capability/bound unavailable".into());
                    } else {
                        let output = registry.bind(name, 4, qh, name);
                        state.outputs.push((name, output, None));
                    }
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                state.outputs.retain(|(id, _, _)| *id != name)
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            if name.len() > 4096 {
                state.error = Some("Output name bound".into());
            } else if let Some(row) = state.outputs.iter_mut().find(|(n, _, _)| n == id) {
                row.2 = Some(name);
            }
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, usize> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            if let (Some(row), Some(buffer)) = (state.rows.get(*id), &state.buffer) {
                row.surface.attach(Some(buffer), 0, 0);
                row.surface.damage_buffer(0, 0, 640, 480);
                row.surface.commit();
            }
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, usize> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure {
            width,
            height,
            states,
        } = event
        {
            let fullscreen = states.chunks_exact(4).any(|bytes| {
                u32::from_ne_bytes(bytes.try_into().expect("four bytes"))
                    == xdg_toplevel::State::Fullscreen as u32
            });
            if *id == 0 {
                state.fullscreen = fullscreen;
                state.configures = state.configures.saturating_add(1);
            }
            println!(
                "{}",
                json!({"configured":id,"width":width,"height":height,"fullscreen":fullscreen})
            );
        }
    }
}
delegate_noop!(State:ignore wl_compositor::WlCompositor);
delegate_noop!(State:ignore wl_shm::WlShm);
delegate_noop!(State:ignore wl_shm_pool::WlShmPool);
delegate_noop!(State:ignore wl_buffer::WlBuffer);
delegate_noop!(State:ignore wl_surface::WlSurface);
delegate_noop!(State:ignore xdg_wm_dialog_v1::XdgWmDialogV1);
delegate_noop!(State:ignore xdg_dialog_v1::XdgDialogV1);
fn run() -> Result<()> {
    let fixed = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("fixed") => true,
        _ => return Err("Unknown client argument".into()),
    };
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let mut state = State::default();
    connection.display().get_registry(&qh, ());
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    if let Some(error) = state.error.take() {
        return Err(error.into());
    }
    let fd =
        unsafe { libc::memfd_create(c"lumen-projection-protocol".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let size = 640 * 480 * 4;
    file.set_len(size)?;
    let mut pixels = vec![0_u8; size as usize];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&0xff8b4a4a_u32.to_ne_bytes());
    }
    file.write_all(&pixels)?;
    let pool = state
        .shm
        .as_ref()
        .ok_or("shm")?
        .create_pool(file.as_fd(), size as i32, &qh, ());
    state.buffer =
        Some(pool.create_buffer(0, 640, 480, 640 * 4, wl_shm::Format::Argb8888, &qh, ()));
    pool.destroy();
    state.window(&qh, 0, fixed, false, "org.example.ProjectionProtocol")?;
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    println!(
        "{}",
        json!({"ready":true,"pid":std::process::id(),"outputs":state.outputs.iter().map(|(_,_,name)|name.clone()).collect::<Vec<_>>()})
    );
    std::io::stdout().flush()?;
    let flags = unsafe { libc::fcntl(0, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(0, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut input = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err("Disposable protocol client lifetime exceeded".into());
        }
        queue.dispatch_pending(&mut state)?;
        connection.flush()?;
        if let Some(error) = state.error.take() {
            return Err(error.into());
        }
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut fds = [
            libc::pollfd {
                fd: connection.as_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, 1000) };
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                drop(guard);
                continue;
            }
            return Err(std::io::Error::last_os_error().into());
        }
        if fds[0].revents & libc::POLLIN != 0 {
            guard.read()?;
        } else {
            drop(guard);
        }
        if fds[1].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            let mut bytes = [0_u8; 128];
            let count = match std::io::stdin().read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(error) => return Err(error.into()),
            };
            input.extend_from_slice(&bytes[..count]);
            if input.len() > 256 {
                return Err("Protocol command input exceeded bound".into());
            }
            while let Some(end) = input.iter().position(|b| *b == b'\n') {
                let line = input.drain(..=end).collect::<Vec<_>>();
                let command = std::str::from_utf8(&line[..end])?;
                if command.len() > 80 {
                    return Err("Protocol command exceeded bound".into());
                }
                if !state.command(&qh, command)? {
                    return Ok(());
                }
                queue.roundtrip(&mut state)?;
                queue.roundtrip(&mut state)?;
                println!(
                    "{}",
                    json!({"ack":command,"fullscreen":state.fullscreen,"configures":state.configures})
                );
                std::io::stdout().flush()?;
            }
        }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Projection protocol client: {error}");
        std::process::exit(1);
    }
}
