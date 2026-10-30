//! Preserve the actual closed foreign resource for a post-closure identity read.
use serde_json::json;
use std::{
    env,
    error::Error,
    fs,
    os::fd::AsRawFd,
    path::PathBuf,
    time::{Duration, Instant},
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, protocol::wl_registry};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as foreign,
};
mod identity {
    use wayland_client;
    use wayland_client::protocol::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols_wlr::foreign_toplevel::v1::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocol.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocol.xml");
}
use identity::{
    lumen_toplevel_identity_manager_v1 as bridge, lumen_toplevel_identity_reply_v1 as reply,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Row {
    handle: handle::ZwlrForeignToplevelHandleV1,
    identifier: Option<String>,
    requested: bool,
}
#[derive(Clone)]
struct Request {
    handle: handle::ZwlrForeignToplevelHandleV1,
    inert: bool,
}
struct State {
    manager: Option<bridge::LumenToplevelIdentityManagerV1>,
    rows: Vec<Row>,
    target: String,
    ready: bool,
    closed: bool,
    inert_result: Option<bool>,
    error: Option<String>,
}
impl State {
    fn request_initial(&mut self, qh: &QueueHandle<Self>) {
        if let Some(manager) = &self.manager {
            for row in &mut self.rows {
                if !row.requested {
                    manager.resolve(
                        &row.handle,
                        qh,
                        Request {
                            handle: row.handle.clone(),
                            inert: false,
                        },
                    );
                    row.requested = true;
                }
            }
        }
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
            if interface == "lumen_toplevel_identity_manager_v1" {
                state.manager = Some(registry.bind(name, version.min(1), qh, ()));
                state.request_initial(qh);
            } else if interface == "zwlr_foreign_toplevel_manager_v1" {
                registry.bind::<foreign::ZwlrForeignToplevelManagerV1, _, _>(
                    name,
                    version.min(3),
                    qh,
                    (),
                );
            }
        }
    }
}
impl Dispatch<foreign::ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &foreign::ZwlrForeignToplevelManagerV1,
        event: foreign::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let foreign::Event::Toplevel { toplevel } = event {
            state.rows.push(Row {
                handle: toplevel,
                identifier: None,
                requested: false,
            });
            state.request_initial(qh);
        }
    }
    wayland_client::event_created_child!(State, foreign::ZwlrForeignToplevelManagerV1, [0 => (handle::ZwlrForeignToplevelHandleV1, ())]);
}
impl Dispatch<handle::ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &handle::ZwlrForeignToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let handle::Event::Closed = event {
            let exact = state.rows.iter().any(|row| {
                row.handle == *proxy && row.identifier.as_deref() == Some(&state.target)
            });
            if exact {
                state.closed = true;
                if let Some(manager) = &state.manager {
                    // closed is not a destructor event in the wlr protocol:
                    // deliberately retain this actual inert resource.
                    manager.resolve(
                        proxy,
                        qh,
                        Request {
                            handle: proxy.clone(),
                            inert: true,
                        },
                    );
                }
            }
        }
    }
}
impl Dispatch<bridge::LumenToplevelIdentityManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &bridge::LumenToplevelIdentityManagerV1,
        _: bridge::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<reply::LumenToplevelIdentityReplyV1, Request> for State {
    fn event(
        state: &mut Self,
        _: &reply::LumenToplevelIdentityReplyV1,
        event: reply::Event,
        request: &Request,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let reply::Event::Result { status, identifier } = event {
            if request.inert {
                state.inert_result = Some(
                    matches!(status, WEnum::Value(reply::Status::Unavailable))
                        && identifier.is_empty(),
                );
            } else if matches!(status, WEnum::Value(reply::Status::Available))
                && !identifier.is_empty()
            {
                if let Some(row) = state
                    .rows
                    .iter_mut()
                    .find(|row| row.handle == request.handle)
                {
                    row.identifier = Some(identifier.clone());
                }
                if identifier == state.target {
                    state.ready = true;
                }
            } else {
                state.error = Some("Live resource did not resolve to a valid identifier".into());
            }
        }
    }
}
fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let target = args.next().ok_or("Expected exact target identifier")?;
    let output = PathBuf::from(args.next().ok_or("Expected disposable output directory")?);
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    fs::create_dir(&output)?;
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State {
        manager: None,
        rows: vec![],
        target: target.clone(),
        ready: false,
        closed: false,
        inert_result: None,
        error: None,
    };
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut published = false;
    loop {
        queue.dispatch_pending(&mut state)?;
        if let Some(error) = &state.error {
            return Err(error.clone().into());
        }
        if state.ready && !published {
            fs::write(
                output.join("ready.json"),
                serde_json::to_vec_pretty(
                    &json!({"ready":true,"identifier":target,"pid":std::process::id(),"scope":"Exact identifier from an actual per-connection foreign handle"}),
                )?,
            )?;
            published = true;
        }
        if let Some(passed) = state.inert_result {
            if !passed || !state.closed {
                return Err("Closed inert resource was accepted or malformed".into());
            }
            fs::write(
                output.join("result.json"),
                serde_json::to_vec_pretty(
                    &json!({"passed":true,"identifier":target,"closed":true,"inertUnavailable":true,"scope":"Actual retained wlr resource resolved after its closed event; no proxy-ID/title/appId join"}),
                )?,
            )?;
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Timed out waiting for exact owned resource closure".into());
        }
        connection.flush()?;
        if let Some(guard) = queue.prepare_read() {
            let mut fd = libc::pollfd {
                fd: connection.as_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut fd, 1, 50) };
            if result < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if result > 0 {
                guard.read()?;
            }
        }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL actual identity wire: {error}");
        std::process::exit(1);
    }
}
