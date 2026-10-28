//! XDND source on a separate X connection. Winit keeps its normal pointer grab.
use anyhow::{Result, ensure};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use x11rb::{
    COPY_FROM_PARENT, CURRENT_TIME, NONE,
    connection::Connection,
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        XdndAware, XdndProxy, XdndEnter, XdndLeave, XdndPosition, XdndStatus,
        XdndDrop, XdndFinished, XdndSelection, XdndActionCopy, XdndActionPrivate, XdndActionList,
        TARGETS, INCR, uri: b"text/uri-list",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Destination {
    window: Window,
    proxy: Window,
    version: u32,
}

struct Increment {
    requestor: Window,
    property: Atom,
    offset: usize,
    touched: Instant,
}

pub fn run(origin: Window, uris: Arc<[u8]>, cancel: Arc<AtomicBool>) -> Result<()> {
    let (connection, _) = x11rb::connect(None)?;
    let atoms = Atoms::new(&connection)?.reply()?;
    let root = connection.get_geometry(origin)?.reply()?.root;
    let source = connection.generate_id()?;
    connection
        .create_window(
            COPY_FROM_PARENT as u8,
            source,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )?
        .check()?;
    connection.change_property32(
        PropMode::REPLACE,
        source,
        atoms.XdndAware,
        AtomEnum::ATOM,
        &[5],
    )?;
    connection.change_property32(
        PropMode::REPLACE,
        source,
        atoms.XdndActionList,
        AtomEnum::ATOM,
        &[atoms.XdndActionCopy, atoms.XdndActionPrivate],
    )?;
    connection
        .set_selection_owner(source, atoms.XdndSelection, CURRENT_TIME)?
        .check()?;
    ensure!(
        connection
            .get_selection_owner(atoms.XdndSelection)?
            .reply()?
            .owner
            == source,
        "Cannot start the file drag"
    );
    connection.flush()?;
    let mut destination = None;
    let mut accepted = false;
    let mut waiting = false;
    let mut last_position = None;
    let mut dropped: Option<Instant> = None;
    let mut released = false;
    let mut released_at = None;
    let mut increments: Vec<Increment> = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            if dropped.is_none()
                && let Some(target) = destination
            {
                message(&connection, target, atoms.XdndLeave, [source, 0, 0, 0, 0])?;
            }
            connection.flush()?;
            return Ok(());
        }
        while let Some(event) = connection.poll_for_event()? {
            match event {
                Event::ClientMessage(event) if event.type_ == atoms.XdndStatus => {
                    let data = event.data.as_data32();
                    if destination.is_some_and(|target: Destination| target.window == data[0]) {
                        waiting = false;
                        // File consumers such as Winit use Private for opening/importing.
                        // Neither offered action authorizes deleting the source files.
                        accepted = data[1] & 1 != 0
                            && (data[4] == atoms.XdndActionCopy
                                || data[4] == atoms.XdndActionPrivate);
                    }
                }
                Event::ClientMessage(event)
                    if event.type_ == atoms.XdndFinished && dropped.is_some() =>
                {
                    if destination.is_some_and(|target| target.window == event.data.as_data32()[0])
                    {
                        return Ok(());
                    }
                }
                Event::SelectionClear(_) => return Ok(()),
                Event::SelectionRequest(request) if request.selection == atoms.XdndSelection => {
                    let property = if request.property == NONE {
                        request.target
                    } else {
                        request.property
                    };
                    let supported = if request.target == atoms.TARGETS {
                        connection
                            .change_property32(
                                PropMode::REPLACE,
                                request.requestor,
                                property,
                                AtomEnum::ATOM,
                                &[atoms.TARGETS, atoms.uri],
                            )
                            .is_ok()
                    } else if request.target == atoms.uri {
                        if uris.len() <= 60 * 1024 {
                            connection
                                .change_property8(
                                    PropMode::REPLACE,
                                    request.requestor,
                                    property,
                                    atoms.uri,
                                    &uris,
                                )
                                .is_ok()
                        } else if increments.len() < 8 {
                            connection.change_window_attributes(
                                request.requestor,
                                &ChangeWindowAttributesAux::new()
                                    .event_mask(EventMask::PROPERTY_CHANGE),
                            )?;
                            connection.change_property32(
                                PropMode::REPLACE,
                                request.requestor,
                                property,
                                atoms.INCR,
                                &[uris.len() as u32],
                            )?;
                            increments.push(Increment {
                                requestor: request.requestor,
                                property,
                                offset: 0,
                                touched: Instant::now(),
                            });
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    connection.send_event(
                        false,
                        request.requestor,
                        EventMask::NO_EVENT,
                        SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: request.time,
                            requestor: request.requestor,
                            selection: request.selection,
                            target: request.target,
                            property: if supported { property } else { NONE },
                        },
                    )?;
                }
                Event::PropertyNotify(event) if event.state == Property::DELETE => {
                    if let Some(index) = increments.iter().position(|transfer| {
                        transfer.requestor == event.window && transfer.property == event.atom
                    }) {
                        let transfer = &mut increments[index];
                        let end = (transfer.offset + 60 * 1024).min(uris.len());
                        connection.change_property8(
                            PropMode::REPLACE,
                            transfer.requestor,
                            transfer.property,
                            atoms.uri,
                            &uris[transfer.offset..end],
                        )?;
                        if transfer.offset == uris.len() {
                            increments.remove(index);
                        } else {
                            transfer.offset = end;
                            transfer.touched = Instant::now();
                        }
                    }
                }
                _ => {}
            }
        }
        increments.retain(|transfer| transfer.touched.elapsed() < Duration::from_secs(30));
        if let Some(time) = dropped {
            ensure!(
                time.elapsed() < Duration::from_secs(30),
                "The destination did not finish receiving the file list"
            );
        } else {
            let pointer = connection.query_pointer(root)?.reply()?;
            released |= !pointer.mask.contains(KeyButMask::BUTTON1);
            if released {
                let since = released_at.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(3) {
                    if let Some(target) = destination {
                        message(&connection, target, atoms.XdndLeave, [source, 0, 0, 0, 0])?;
                    }
                    connection.flush()?;
                    return Ok(());
                }
            }
            let position = (pointer.root_x, pointer.root_y);
            let target = destination_at(&connection, &atoms, root, origin)?;
            if target != destination {
                if let Some(old) = destination {
                    message(&connection, old, atoms.XdndLeave, [source, 0, 0, 0, 0])?;
                }
                destination = target;
                accepted = false;
                waiting = false;
                last_position = None;
                if let Some(target) = target {
                    message(
                        &connection,
                        target,
                        atoms.XdndEnter,
                        [source, target.version.min(5) << 24, atoms.uri, 0, 0],
                    )?;
                }
            }
            if let Some(target) = destination {
                if !waiting && last_position != Some(position) {
                    let packed = ((position.0 as u16 as u32) << 16) | position.1 as u16 as u32;
                    message(
                        &connection,
                        target,
                        atoms.XdndPosition,
                        [source, 0, packed, CURRENT_TIME, atoms.XdndActionCopy],
                    )?;
                    waiting = true;
                    last_position = Some(position);
                } else if released && !waiting {
                    if accepted {
                        message(
                            &connection,
                            target,
                            atoms.XdndDrop,
                            [source, 0, CURRENT_TIME, 0, 0],
                        )?;
                        dropped = Some(Instant::now());
                    } else {
                        message(&connection, target, atoms.XdndLeave, [source, 0, 0, 0, 0])?;
                        connection.flush()?;
                        return Ok(());
                    }
                }
            } else if released {
                return Ok(());
            }
        }
        connection.flush()?;
        std::thread::sleep(Duration::from_millis(12));
    }
}

fn message(
    connection: &RustConnection,
    target: Destination,
    kind: Atom,
    data: [u32; 5],
) -> Result<()> {
    connection.send_event(
        false,
        target.proxy,
        EventMask::NO_EVENT,
        ClientMessageEvent::new(32, target.window, kind, data),
    )?;
    Ok(())
}

fn property(
    connection: &RustConnection,
    window: Window,
    property: Atom,
    kind: AtomEnum,
) -> Option<u32> {
    connection
        .get_property(false, window, property, kind, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

fn destination_at(
    connection: &RustConnection,
    atoms: &Atoms,
    root: Window,
    origin: Window,
) -> Result<Option<Destination>> {
    let mut window = root;
    let mut destination = None;
    for _ in 0..32 {
        if window == origin {
            return Ok(None);
        }
        let proxy = property(connection, window, atoms.XdndProxy, AtomEnum::WINDOW)
            .filter(|proxy| {
                property(connection, *proxy, atoms.XdndProxy, AtomEnum::WINDOW) == Some(*proxy)
            })
            .unwrap_or(window);
        if let Some(version) = property(connection, proxy, atoms.XdndAware, AtomEnum::ATOM)
            && version >= 3
        {
            destination = Some(Destination {
                window,
                proxy,
                version,
            });
        }
        let child = connection.query_pointer(window)?.reply()?.child;
        if child == NONE {
            break;
        }
        window = child;
    }
    Ok(destination)
}
