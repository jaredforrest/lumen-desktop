//! Disposable acknowledged X11 protocol client, shared by relationship/projection tests.
use serde_json::json;
use std::error::Error;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Atoms {
    transient: Atom,
    state: Atom,
    modal: Atom,
    fullscreen: Atom,
    pid: Atom,
    protocols: Atom,
    delete: Atom,
}
struct Client {
    connection: RustConnection,
    screen: Screen,
    atoms: Atoms,
    windows: [Window; 4],
    keys: [u64; 4],
    updates: [u64; 4],
    serial: u64,
    xres: bool,
    label: String,
}
impl Client {
    fn new(label: String) -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let screen = connection.setup().roots[screen].clone();
        let atom = |name: &str| -> Result<Atom> {
            Ok(connection
                .intern_atom(false, name.as_bytes())?
                .reply()?
                .atom)
        };
        let atoms = Atoms {
            transient: atom("WM_TRANSIENT_FOR")?,
            state: atom("_NET_WM_STATE")?,
            modal: atom("_NET_WM_STATE_MODAL")?,
            fullscreen: atom("_NET_WM_STATE_FULLSCREEN")?,
            pid: atom("_NET_WM_PID")?,
            protocols: atom("WM_PROTOCOLS")?,
            delete: atom("WM_DELETE_WINDOW")?,
        };
        let xres = connection.query_extension(b"X-Resource")?.reply()?.present;
        let mut client = Self {
            connection,
            screen,
            atoms,
            windows: [0; 4],
            keys: [0; 4],
            updates: [0; 4],
            serial: 0,
            xres,
            label,
        };
        for index in 0..4 {
            client.create(index)?;
        }
        client.connection.flush()?;
        client.status()?;
        Ok(client)
    }
    fn create(&mut self, index: usize) -> Result<()> {
        let window = self.connection.generate_id()?;
        self.windows[index] = window;
        self.connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            self.screen.root,
            (80 + index * 40) as i16,
            (100 + index * 35) as i16,
            320,
            220,
            0,
            WindowClass::INPUT_OUTPUT,
            self.screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(self.screen.white_pixel)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::KEY_PRESS
                        | EventMask::STRUCTURE_NOTIFY
                        | EventMask::PROPERTY_CHANGE,
                ),
        )?;
        self.connection.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            format!("Synthetic X11 {} {index}", self.label).as_bytes(),
        )?;
        self.connection.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"fixture\0org.example.Relationships\0",
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms.protocols,
            AtomEnum::ATOM,
            &[self.atoms.delete],
        )?;
        Ok(())
    }
    fn status(&self) -> Result<()> {
        let mut windows = Vec::new();
        for (index, &window) in self.windows.iter().enumerate() {
            let attributes = if window != 0 {
                self.connection.get_window_attributes(window)?.reply().ok()
            } else {
                None
            };
            let states = if window != 0 {
                self.connection
                    .get_property(false, window, self.atoms.state, AtomEnum::ATOM, 0, 128)?
                    .reply()
                    .ok()
            } else {
                None
            };
            let fullscreen = states
                .as_ref()
                .and_then(|s| s.value32())
                .is_some_and(|mut atoms| atoms.any(|a| a == self.atoms.fullscreen));
            windows.push(json!({"id":window,"keys":self.keys[index],"mapState":attributes.as_ref().map_or(-1,|a|u8::from(a.map_state) as i32),"overrideRedirect":attributes.as_ref().map_or(-1,|a|i32::from(a.override_redirect)),"fullscreen":fullscreen,"stateUpdates":self.updates[index]}));
        }
        println!(
            "{}",
            json!({"serial":self.serial,"pid":std::process::id(),"xres":self.xres,"windows":windows})
        );
        io::stdout().flush()?;
        Ok(())
    }
    fn events(&mut self) -> Result<bool> {
        loop {
            let event = match self.connection.poll_for_event() {
                Ok(Some(event)) => event,
                Ok(None) => return Ok(true),
                Err(_) => return Ok(false),
            };
            match event {
                Event::KeyPress(key) => {
                    for (index, &window) in self.windows.iter().enumerate() {
                        if window == key.event {
                            self.keys[index] += 1;
                        }
                    }
                    self.status()?;
                }
                Event::PropertyNotify(property) if property.atom == self.atoms.state => {
                    for (index, &window) in self.windows.iter().enumerate() {
                        if window == property.window {
                            self.updates[index] += 1;
                        }
                    }
                    self.status()?;
                }
                Event::ClientMessage(message)
                    if message.type_ == self.atoms.protocols
                        && message.data.as_data32()[0] == self.atoms.delete =>
                {
                    for window in &mut self.windows {
                        if *window == message.window {
                            self.connection.destroy_window(*window)?;
                            *window = 0;
                        }
                    }
                    self.connection.flush()?;
                    self.status()?;
                }
                _ => {}
            }
        }
    }
    fn command(&mut self, line: &str) -> Result<bool> {
        let mut parts = line.split_whitespace();
        let Some(action) = parts.next() else {
            return Ok(false);
        };
        let index = parts.next().unwrap_or("0").parse::<usize>()?;
        let value = parts.next().unwrap_or("0").parse::<u32>()?;
        if index >= 4 || action == "exit" {
            return Ok(false);
        }
        let window = self.windows[index];
        if action == "new" {
            if window != 0 {
                return Ok(false);
            }
            self.create(index)?;
        } else if window == 0 {
            return Ok(false);
        } else {
            match action {
                "map" => {
                    self.connection.map_window(window)?;
                }
                "unmap" => {
                    self.connection.unmap_window(window)?;
                }
                "destroy" => {
                    self.connection.destroy_window(window)?;
                    self.windows[index] = 0;
                }
                "parent" => {
                    if value != 0 {
                        self.connection.change_property32(
                            PropMode::REPLACE,
                            window,
                            self.atoms.transient,
                            AtomEnum::WINDOW,
                            &[value],
                        )?;
                    } else {
                        self.connection
                            .delete_property(window, self.atoms.transient)?;
                    }
                }
                "pid" => {
                    self.connection.change_property32(
                        PropMode::REPLACE,
                        window,
                        self.atoms.pid,
                        AtomEnum::CARDINAL,
                        &[value],
                    )?;
                }
                "override" => {
                    self.connection.change_window_attributes(
                        window,
                        &ChangeWindowAttributesAux::new().override_redirect(value),
                    )?;
                    self.connection.configure_window(
                        window,
                        &ConfigureWindowAux::new()
                            .x((80 + index * 40 + usize::from(value != 0)) as i32),
                    )?;
                }
                "property" => {
                    if value != 0 {
                        self.connection.change_property32(
                            PropMode::REPLACE,
                            window,
                            self.atoms.state,
                            AtomEnum::ATOM,
                            &[self.atoms.modal],
                        )?;
                    } else {
                        self.connection.delete_property(window, self.atoms.state)?;
                    }
                }
                "modal" | "fullscreen" => {
                    let atom = if action == "modal" {
                        self.atoms.modal
                    } else {
                        self.atoms.fullscreen
                    };
                    let message = ClientMessageEvent::new(
                        32,
                        window,
                        self.atoms.state,
                        [u32::from(value != 0), atom, 0, 1, 0],
                    );
                    self.connection.send_event(
                        false,
                        self.screen.root,
                        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                        message,
                    )?;
                }
                _ => return Ok(false),
            }
        }
        self.connection.flush()?;
        self.connection.get_input_focus()?.reply()?;
        self.serial += 1;
        self.status()?;
        Ok(true)
    }
    fn run(&mut self) -> Result<()> {
        let mut buffer = Vec::new();
        loop {
            if !self.events()? {
                return Ok(());
            }
            let mut descriptors = [
                libc::pollfd {
                    fd: 0,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.connection.stream().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if descriptors[0].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
                let mut chunk = [0u8; 256];
                let count = unsafe { libc::read(0, chunk.as_mut_ptr().cast(), chunk.len()) };
                if count == 0 {
                    return Ok(());
                }
                if count < 0 {
                    return Err(io::Error::last_os_error().into());
                }
                buffer.extend_from_slice(&chunk[..count as usize]);
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    if end >= 256 {
                        return Err("X11 command exceeds bound".into());
                    }
                    let line = buffer.drain(..=end).collect::<Vec<_>>();
                    if !self.command(std::str::from_utf8(&line)?)? {
                        return Ok(());
                    }
                }
                if buffer.len() >= 256 {
                    return Err("X11 command exceeds bound".into());
                }
            }
        }
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 1 || args[0].len() > 80 {
        std::process::exit(2)
    }
    let mut client = match Client::new(args[0].clone()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = client.run() {
        // The existing protocol fixture exits cleanly when its owned X server
        // closes, including closure during a pending status/ack roundtrip.
        if error
            .downcast_ref::<x11rb::errors::ConnectionError>()
            .is_some()
            || matches!(
                error.downcast_ref::<x11rb::errors::ReplyError>(),
                Some(x11rb::errors::ReplyError::ConnectionError(_))
            )
        {
            return;
        }
        eprintln!("{error}");
        std::process::exit(2)
    }
}
