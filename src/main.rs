mod args;
mod device;
mod engine;
mod osc;
mod plugin;

use anyhow::{Context, Result};
use clap::Parser;
use cpal::traits::DeviceTrait;
use std::collections::HashSet;

use args::Args;
use device::{get_cpal_host, get_device_config, print_devices, select_device};
use engine::{AudioEngine, MainThreadMessage, OscClapHost, OscClapHostMainThread, OscClapHostShared};
use osc::{create_command_queue, start_osc_receiver};
use plugin::{enumerate_params, load_bundle, print_osc_api, print_plugins, select_plugin_id};

use clack_host::prelude::*;
use crossbeam_channel::unbounded;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    let cpal_host = get_cpal_host();

    if args.list_devices {
        return print_devices(&cpal_host);
    }

    let plugin_path = args
        .plugin_path
        .as_ref()
        .context("Plugin path is required")?;

    let bundle = load_bundle(plugin_path)?;

    if args.list_plugins {
        return print_plugins(&bundle);
    }

    let plugin_id = select_plugin_id(
        &bundle,
        args.plugin_id.as_deref(),
        args.plugin_index,
    )?;

    log::info!("Loading plugin: {:?}", plugin_id);

    let host_info = HostInfo::new(
        "OSC CLAP Host",
        "OSC CLAP Host",
        "https://github.com/example/osc-clap-host",
        "0.1.0",
    )?;

    let (sender, _receiver) = unbounded();

    let mut instance = PluginInstance::<OscClapHost>::new(
        |_| OscClapHostShared::new(sender.clone()),
        |shared| OscClapHostMainThread::new(shared),
        &bundle,
        &plugin_id,
        &host_info,
    )?;

    let params = enumerate_params(&mut instance);

    if args.print_osc {
        print_osc_api(&params);
        return Ok(());
    }

    let per_note_mod_params: HashSet<u32> = params
        .iter()
        .filter(|p| p.is_modulatable_per_note_id)
        .map(|p| p.id)
        .collect();

    let device = select_device(&cpal_host, args.device)?;
    log::info!("Using audio device: {}", device.name().unwrap_or_default());

    let audio_config = get_device_config(
        &device,
        args.sample_rate,
        args.channels,
        args.buffer_size,
    )?;

    log::info!(
        "Audio config: {}Hz, {} channels, buffer size {}",
        audio_config.sample_rate,
        audio_config.channels,
        audio_config.buffer_size
    );

    let plugin_audio_config = PluginAudioConfiguration {
        sample_rate: audio_config.sample_rate as f64,
        min_frames_count: 1,
        max_frames_count: audio_config.buffer_size,
    };

    let stopped_processor = instance.activate(|_, _| (), plugin_audio_config)?;
    let audio_processor = stopped_processor
        .start_processing()
        .map_err(|e| anyhow::anyhow!("Failed to start processing: {:?}", e))?;

    let (command_producer, command_consumer) = create_command_queue(1024);

    let _osc_handle = start_osc_receiver(args.osc_port, command_producer, per_note_mod_params)?;

    let cpal_config = cpal::StreamConfig {
        channels: audio_config.channels,
        sample_rate: cpal::SampleRate(audio_config.sample_rate),
        buffer_size: cpal::BufferSize::Fixed(audio_config.buffer_size),
    };

    let (_engine, main_receiver) = AudioEngine::new(
        &device,
        cpal_config,
        audio_config.sample_format,
        audio_processor,
        command_consumer,
        audio_config.channels as usize,
        audio_config.buffer_size as usize * 2,
    )?;

    log::info!(
        "OSC CLAP Host running. Listening for OSC on 127.0.0.1:{}",
        args.osc_port
    );
    log::info!("Press Ctrl+C to stop.");

    for message in main_receiver {
        match message {
            MainThreadMessage::RunOnMainThread => {
                instance.call_on_main_thread_callback();
            }
        }
    }

    Ok(())
}
