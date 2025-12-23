use crate::osc::Command;
use anyhow::{Context, Result};
use clack_extensions::audio_ports::{HostAudioPortsImpl, RescanType};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::note_ports::{HostNotePortsImpl, NoteDialects, NotePortRescanFlags};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_host::prelude::*;
use clack_host::process::StartedPluginAudioProcessor;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BuildStreamError, Device, FromSample, OutputCallbackInfo, Sample, SampleFormat, Stream, StreamConfig};
use crossbeam_channel::{Receiver, Sender, unbounded};
use rtrb::Consumer;
use std::sync::OnceLock;

pub struct OscClapHost;

impl HostHandlers for OscClapHost {
    type Shared<'a> = OscClapHostShared;
    type MainThread<'a> = OscClapHostMainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostLog>()
            .register::<HostParams>();
    }
}

pub enum MainThreadMessage {
    RunOnMainThread,
}

pub struct OscClapHostShared {
    sender: Sender<MainThreadMessage>,
    callbacks: OnceLock<()>,
}

impl OscClapHostShared {
    pub fn new(sender: Sender<MainThreadMessage>) -> Self {
        Self {
            sender,
            callbacks: OnceLock::new(),
        }
    }
}

impl<'a> SharedHandler<'a> for OscClapHostShared {
    fn initializing(&self, _instance: InitializingPluginHandle<'a>) {
        let _ = self.callbacks.set(());
    }

    fn request_restart(&self) {}

    fn request_process(&self) {}

    fn request_callback(&self) {
        let _ = self.sender.send(MainThreadMessage::RunOnMainThread);
    }
}

pub struct OscClapHostMainThread<'a> {
    _shared: &'a OscClapHostShared,
    _plugin: Option<InitializedPluginHandle<'a>>,
}

impl<'a> OscClapHostMainThread<'a> {
    pub fn new(shared: &'a OscClapHostShared) -> Self {
        Self {
            _shared: shared,
            _plugin: None,
        }
    }
}

impl<'a> MainThreadHandler<'a> for OscClapHostMainThread<'a> {
    fn initialized(&mut self, instance: InitializedPluginHandle<'a>) {
        self._plugin = Some(instance);
    }
}

impl HostLogImpl for OscClapHostShared {
    fn log(&self, severity: LogSeverity, message: &str) {
        match severity {
            LogSeverity::Debug => log::debug!("[plugin] {}", message),
            LogSeverity::Info => log::info!("[plugin] {}", message),
            LogSeverity::Warning => log::warn!("[plugin] {}", message),
            LogSeverity::Error => log::error!("[plugin] {}", message),
            LogSeverity::Fatal => log::error!("[plugin FATAL] {}", message),
            LogSeverity::HostMisbehaving => log::error!("[plugin HOST_MISBEHAVING] {}", message),
            LogSeverity::PluginMisbehaving => log::warn!("[PLUGIN_MISBEHAVING] {}", message),
        }
    }
}

impl HostAudioPortsImpl for OscClapHostMainThread<'_> {
    fn is_rescan_flag_supported(&self, _flag: RescanType) -> bool {
        false
    }

    fn rescan(&mut self, _flag: RescanType) {}
}

impl HostNotePortsImpl for OscClapHostMainThread<'_> {
    fn supported_dialects(&self) -> NoteDialects {
        NoteDialects::CLAP
    }

    fn rescan(&mut self, _flags: NotePortRescanFlags) {}
}

impl HostParamsImplMainThread for OscClapHostMainThread<'_> {
    fn rescan(&mut self, _flags: ParamRescanFlags) {}
    fn clear(&mut self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostParamsImplShared for OscClapHostShared {
    fn request_flush(&self) {}
}

pub struct AudioEngine {
    stream: Stream,
    _receiver: Receiver<MainThreadMessage>,
    _sender: Sender<MainThreadMessage>,  // Keep sender alive to prevent receiver from disconnecting
}

impl AudioEngine {
    pub fn new(
        device: &Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        audio_processor: StartedPluginAudioProcessor<OscClapHost>,
        command_consumer: Consumer<Command>,
        channel_count: usize,
        max_buffer_size: usize,
        verbose: bool,
    ) -> Result<(Self, Receiver<MainThreadMessage>)> {
        let (sender, receiver) = unbounded();

        let processor = StreamAudioProcessor::new(
            audio_processor,
            command_consumer,
            channel_count,
            max_buffer_size,
            verbose,
        );

        let stream = build_output_stream_for_sample_format(device, processor, &config, sample_format)?;
        stream.play().context("Failed to start audio stream")?;

        Ok((
            Self {
                stream,
                _receiver: receiver.clone(),
                _sender: sender,  // Store sender to keep it alive
            },
            receiver,
        ))
    }

    pub fn stream(&self) -> &Stream {
        &self.stream
    }
}

fn build_output_stream_for_sample_format(
    device: &Device,
    processor: StreamAudioProcessor,
    config: &StreamConfig,
    sample_format: SampleFormat,
) -> Result<Stream, BuildStreamError> {
    let err = |e| log::error!("Audio stream error: {}", e);

    match sample_format {
        SampleFormat::I8 => device.build_output_stream(config, make_stream_runner::<i8>(processor), err, None),
        SampleFormat::I16 => device.build_output_stream(config, make_stream_runner::<i16>(processor), err, None),
        SampleFormat::I32 => device.build_output_stream(config, make_stream_runner::<i32>(processor), err, None),
        SampleFormat::U8 => device.build_output_stream(config, make_stream_runner::<u8>(processor), err, None),
        SampleFormat::U16 => device.build_output_stream(config, make_stream_runner::<u16>(processor), err, None),
        SampleFormat::U32 => device.build_output_stream(config, make_stream_runner::<u32>(processor), err, None),
        SampleFormat::F32 => device.build_output_stream(config, make_stream_runner::<f32>(processor), err, None),
        SampleFormat::F64 => device.build_output_stream(config, make_stream_runner::<f64>(processor), err, None),
        _ => device.build_output_stream(config, make_stream_runner::<f32>(processor), err, None),
    }
}

fn make_stream_runner<S: FromSample<f32> + Sample>(
    mut audio_processor: StreamAudioProcessor,
) -> impl FnMut(&mut [S], &OutputCallbackInfo) {
    move |data, _info| audio_processor.process(data)
}

struct StreamAudioProcessor {
    audio_processor: StartedPluginAudioProcessor<OscClapHost>,
    command_consumer: Consumer<Command>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    input_buffers: Vec<f32>,
    output_buffers: Vec<f32>,
    channel_count: usize,
    steady_counter: u64,
    verbose: bool,
}

impl StreamAudioProcessor {
    fn new(
        audio_processor: StartedPluginAudioProcessor<OscClapHost>,
        command_consumer: Consumer<Command>,
        channel_count: usize,
        max_buffer_size: usize,
        verbose: bool,
    ) -> Self {
        Self {
            audio_processor,
            command_consumer,
            input_ports: AudioPorts::with_capacity(channel_count, 1),
            output_ports: AudioPorts::with_capacity(channel_count, 1),
            input_buffers: vec![0.0; channel_count * max_buffer_size],
            output_buffers: vec![0.0; channel_count * max_buffer_size],
            channel_count,
            steady_counter: 0,
            verbose,
        }
    }

    fn process<S: FromSample<f32> + Sample>(&mut self, data: &mut [S]) {
        let frame_count = data.len() / self.channel_count;
        let needed_size = self.channel_count * frame_count;

        if self.output_buffers.len() < needed_size {
            self.input_buffers.resize(needed_size, 0.0);
            self.output_buffers.resize(needed_size, 0.0);
        }

        self.input_buffers[..needed_size].fill(0.0);
        self.output_buffers[..needed_size].fill(0.0);

        let mut input_event_buffer = EventBuffer::new();
        let mut event_count = 0;
        while let Ok(cmd) = self.command_consumer.pop() {
            if self.verbose {
                log::info!("[AUDIO-DEQUEUE] Processing command: {:?}", cmd);
            }
            if let Some(event) = command_to_event(cmd.clone()) {
                event_count += 1;
                if self.verbose {
                    log::info!("[AUDIO-EVENT] Sending to plugin: {:?}", format_event(&event));
                }
                match event {
                    EventUnion::NoteOn(e) => { input_event_buffer.push(&e); }
                    EventUnion::NoteOff(e) => { input_event_buffer.push(&e); }
                    EventUnion::NoteChoke(e) => { input_event_buffer.push(&e); }
                    EventUnion::ParamValue(e) => { input_event_buffer.push(&e); }
                    EventUnion::ParamMod(e) => { input_event_buffer.push(&e); }
                }
            }
        }
        if self.verbose && event_count > 0 {
            log::info!("[AUDIO-PROCESS] Processing {} events, {} frames", event_count, frame_count);
        }

        let input_events_ref = InputEvents::from_buffer(&input_event_buffer);

        let mut output_events = EventBuffer::new();
        let mut output_events_ref = OutputEvents::from_buffer(&mut output_events);

        let channel_frame_count = frame_count;
        let mut input_channels: Vec<&mut [f32]> = self.input_buffers[..needed_size]
            .chunks_exact_mut(channel_frame_count)
            .take(self.channel_count)
            .collect();
        let mut output_channels: Vec<&mut [f32]> = self.output_buffers[..needed_size]
            .chunks_exact_mut(channel_frame_count)
            .take(self.channel_count)
            .collect();

        let inputs = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                input_channels.iter_mut().map(|ch| InputChannel {
                    buffer: *ch,
                    is_constant: true,
                }),
            ),
        }]);

        let mut outputs = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                output_channels.iter_mut().map(|ch| &mut **ch),
            ),
        }]);

        match self.audio_processor.process(
            &inputs,
            &mut outputs,
            &input_events_ref,
            &mut output_events_ref,
            Some(self.steady_counter),
            None,
        ) {
            Ok(_) => {
                interleave_to_output(data, &self.output_buffers, self.channel_count, frame_count);
            }
            Err(e) => {
                log::error!("Plugin process error: {:?}", e);
                for sample in data.iter_mut() {
                    *sample = S::EQUILIBRIUM;
                }
            }
        }

        self.steady_counter += frame_count as u64;
    }
}

enum EventUnion {
    NoteOn(NoteOnEvent),
    NoteOff(NoteOffEvent),
    NoteChoke(NoteChokeEvent),
    ParamValue(ParamValueEvent),
    ParamMod(ParamModEvent),
}

fn format_event(event: &EventUnion) -> String {
    match event {
        EventUnion::NoteOn(_) => "NoteOn".to_string(),
        EventUnion::NoteOff(_) => "NoteOff".to_string(),
        EventUnion::NoteChoke(_) => "NoteChoke".to_string(),
        EventUnion::ParamValue(_) => "ParamValue".to_string(),
        EventUnion::ParamMod(_) => "ParamMod".to_string(),
    }
}

impl AsRef<UnknownEvent> for EventUnion {
    fn as_ref(&self) -> &UnknownEvent {
        match self {
            EventUnion::NoteOn(e) => e.as_ref(),
            EventUnion::NoteOff(e) => e.as_ref(),
            EventUnion::NoteChoke(e) => e.as_ref(),
            EventUnion::ParamValue(e) => e.as_ref(),
            EventUnion::ParamMod(e) => e.as_ref(),
        }
    }
}

fn command_to_event(cmd: Command) -> Option<EventUnion> {
    match cmd {
        Command::NoteOn {
            note_id,
            key,
            velocity,
            channel,
            port,
        } => {
            let pckn = Pckn::new(port as u16, channel as u16, key as u16, note_id as u32);
            Some(EventUnion::NoteOn(NoteOnEvent::new(0, pckn, velocity as f64)))
        }
        Command::NoteOff {
            note_id,
            key,
            velocity,
            channel,
            port,
        } => {
            let pckn = Pckn::new(port as u16, channel as u16, key as u16, note_id as u32);
            Some(EventUnion::NoteOff(NoteOffEvent::new(0, pckn, velocity as f64)))
        }
        Command::NoteChoke {
            note_id,
            key,
            channel,
            port,
        } => {
            let pckn = Pckn::new(port as u16, channel as u16, key as u16, note_id as u32);
            Some(EventUnion::NoteChoke(NoteChokeEvent::new(0, pckn)))
        }
        Command::ParamSet { param_id, value } => {
            let param_id = ClapId::from_raw(param_id)?;
            let pckn = Pckn::new(Match::All, Match::All, Match::All, Match::All);
            Some(EventUnion::ParamValue(ParamValueEvent::new(
                0,
                param_id,
                pckn,
                value,
                Cookie::empty(),
            )))
        }
        Command::ParamMod {
            note_id,
            param_id,
            amount,
            key,
            channel,
            port,
        } => {
            let param_id = ClapId::from_raw(param_id)?;
            let pckn = if note_id < 0 {
                Pckn::new(Match::All, Match::All, Match::All, Match::All)
            } else {
                let port_match = if port < 0 { Match::All } else { Match::Specific(port as u16) };
                let chan_match = if channel < 0 { Match::All } else { Match::Specific(channel as u16) };
                let key_match = if key < 0 { Match::All } else { Match::Specific(key as u16) };
                Pckn::new(port_match, chan_match, key_match, Match::Specific(note_id as u32))
            };
            Some(EventUnion::ParamMod(ParamModEvent::new(
                0,
                param_id,
                pckn,
                amount,
                Cookie::empty(),
            )))
        }
    }
}

fn interleave_to_output<S: FromSample<f32> + Sample>(
    output: &mut [S],
    channel_buffers: &[f32],
    channel_count: usize,
    frame_count: usize,
) {
    for frame in 0..frame_count {
        for ch in 0..channel_count {
            let src_idx = ch * frame_count + frame;
            let dst_idx = frame * channel_count + ch;
            if src_idx < channel_buffers.len() && dst_idx < output.len() {
                output[dst_idx] = S::from_sample(channel_buffers[src_idx]);
            }
        }
    }
}

use clack_host::events::event_types::{
    NoteChokeEvent, NoteOffEvent, NoteOnEvent, ParamModEvent, ParamValueEvent,
};
use clack_host::events::io::EventBuffer;
use clack_host::events::{Match, Pckn, UnknownEvent};
use clack_host::prelude::{AudioPortBuffer, AudioPortBufferType, AudioPorts, InputChannel};
use clack_host::utils::{ClapId, Cookie};
