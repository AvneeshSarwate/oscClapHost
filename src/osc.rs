use anyhow::{Context, Result};
use rosc::{OscMessage, OscPacket, OscType};
use rtrb::{Producer, RingBuffer};
use std::collections::HashSet;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone)]
pub enum Command {
    NoteOn {
        note_id: i32,
        key: i32,
        velocity: f32,
        channel: i32,
        port: i32,
    },
    NoteOff {
        note_id: i32,
        key: i32,
        velocity: f32,
        channel: i32,
        port: i32,
    },
    NoteChoke {
        note_id: i32,
        key: i32,
        channel: i32,
        port: i32,
    },
    ParamSet {
        param_id: u32,
        value: f64,
    },
    ParamMod {
        note_id: i32,
        param_id: u32,
        amount: f64,
        key: i32,
        channel: i32,
        port: i32,
    },
    DumpPatchState,
}

pub fn create_command_queue(capacity: usize) -> (Producer<Command>, rtrb::Consumer<Command>) {
    RingBuffer::new(capacity)
}

pub fn start_osc_receiver(
    port: u16,
    mut producer: Producer<Command>,
    per_note_mod_params: HashSet<u32>,
    verbose: bool,
) -> Result<thread::JoinHandle<()>> {
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", port))
        .context(format!("Failed to bind OSC socket on port {}", port))?;

    log::info!("OSC receiver listening on 127.0.0.1:{}", port);

    let handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];

        loop {
            match socket.recv_from(&mut buf) {
                Ok((size, addr)) => {
                    if verbose {
                        log::info!("[OSC-RECV] Received {} bytes from {}", size, addr);
                    }
                    if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                        process_packet(&packet, &mut producer, &per_note_mod_params, verbose);
                    }
                }
                Err(e) => {
                    log::error!("OSC receive error: {}", e);
                }
            }
        }
    });

    Ok(handle)
}

fn process_packet(
    packet: &OscPacket,
    producer: &mut Producer<Command>,
    per_note_mod_params: &HashSet<u32>,
    verbose: bool,
) {
    match packet {
        OscPacket::Message(msg) => {
            if verbose {
                log::info!("[OSC-PARSE] Message: {} args={:?}", msg.addr, msg.args);
            }
            if let Some(cmd) = parse_message(msg, per_note_mod_params) {
                if verbose {
                    log::info!("[OSC-QUEUE] Pushing command: {:?}", cmd);
                }
                if producer.push(cmd).is_err() {
                    log::warn!("Command queue full, dropping OSC message");
                }
            }
        }
        OscPacket::Bundle(bundle) => {
            for p in &bundle.content {
                process_packet(p, producer, per_note_mod_params, verbose);
            }
        }
    }
}

fn parse_message(msg: &OscMessage, per_note_mod_params: &HashSet<u32>) -> Option<Command> {
    match msg.addr.as_str() {
        "/note/on" => parse_note_on(&msg.args),
        "/note/off" => parse_note_off(&msg.args),
        "/note/choke" => parse_note_choke(&msg.args),
        "/param/set" => parse_param_set(&msg.args),
        "/param/mod" => parse_param_mod(&msg.args, per_note_mod_params),
        "/patchState" => Some(Command::DumpPatchState),
        _ => {
            log::debug!("Unknown OSC address: {}", msg.addr);
            None
        }
    }
}

fn parse_note_on(args: &[OscType]) -> Option<Command> {
    if args.len() < 3 {
        log::warn!("/note/on requires at least 3 args: note_id, key, vel");
        return None;
    }

    let note_id = get_i32(&args[0])?;
    let key = get_i32(&args[1])?;
    let velocity = get_f32(&args[2])?;
    let channel = args.get(3).and_then(get_i32).unwrap_or(0);
    let port = args.get(4).and_then(get_i32).unwrap_or(0);

    Some(Command::NoteOn {
        note_id,
        key,
        velocity,
        channel,
        port,
    })
}

fn parse_note_off(args: &[OscType]) -> Option<Command> {
    if args.len() < 3 {
        log::warn!("/note/off requires at least 3 args: note_id, key, vel");
        return None;
    }

    let note_id = get_i32(&args[0])?;
    let key = get_i32(&args[1])?;
    let velocity = get_f32(&args[2])?;
    let channel = args.get(3).and_then(get_i32).unwrap_or(0);
    let port = args.get(4).and_then(get_i32).unwrap_or(0);

    Some(Command::NoteOff {
        note_id,
        key,
        velocity,
        channel,
        port,
    })
}

fn parse_note_choke(args: &[OscType]) -> Option<Command> {
    if args.is_empty() {
        log::warn!("/note/choke requires at least 1 arg: note_id");
        return None;
    }

    let note_id = get_i32(&args[0])?;
    let key = args.get(1).and_then(get_i32).unwrap_or(-1);
    let channel = args.get(2).and_then(get_i32).unwrap_or(-1);
    let port = args.get(3).and_then(get_i32).unwrap_or(-1);

    Some(Command::NoteChoke {
        note_id,
        key,
        channel,
        port,
    })
}

fn parse_param_set(args: &[OscType]) -> Option<Command> {
    if args.len() < 2 {
        log::warn!("/param/set requires 2 args: param_id, value");
        return None;
    }

    let param_id = get_u32(&args[0])?;
    let value = get_f64(&args[1])?;

    Some(Command::ParamSet { param_id, value })
}

fn parse_param_mod(args: &[OscType], per_note_mod_params: &HashSet<u32>) -> Option<Command> {
    if args.len() < 3 {
        log::warn!("/param/mod requires at least 3 args: note_id, param_id, amount");
        return None;
    }

    let note_id = get_i32(&args[0])?;
    let param_id = get_u32(&args[1])?;
    let amount = get_f64(&args[2])?;

    if !per_note_mod_params.contains(&param_id) {
        log::warn!(
            "Parameter {} does not support per-note modulation, ignoring /param/mod",
            param_id
        );
        return None;
    }

    let key = args.get(3).and_then(get_i32).unwrap_or(-1);
    let channel = args.get(4).and_then(get_i32).unwrap_or(-1);
    let port = args.get(5).and_then(get_i32).unwrap_or(-1);

    Some(Command::ParamMod {
        note_id,
        param_id,
        amount,
        key,
        channel,
        port,
    })
}

fn get_i32(arg: &OscType) -> Option<i32> {
    match arg {
        OscType::Int(v) => Some(*v),
        OscType::Long(v) => Some(*v as i32),
        OscType::Float(v) => Some(*v as i32),
        OscType::Double(v) => Some(*v as i32),
        _ => None,
    }
}

fn get_u32(arg: &OscType) -> Option<u32> {
    match arg {
        OscType::Int(v) => Some(*v as u32),
        OscType::Long(v) => Some(*v as u32),
        OscType::Float(v) => Some(*v as u32),
        OscType::Double(v) => Some(*v as u32),
        _ => None,
    }
}

fn get_f32(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(v) => Some(*v),
        OscType::Double(v) => Some(*v as f32),
        OscType::Int(v) => Some(*v as f32),
        OscType::Long(v) => Some(*v as f32),
        _ => None,
    }
}

fn get_f64(arg: &OscType) -> Option<f64> {
    match arg {
        OscType::Double(v) => Some(*v),
        OscType::Float(v) => Some(*v as f64),
        OscType::Int(v) => Some(*v as f64),
        OscType::Long(v) => Some(*v as f64),
        _ => None,
    }
}
