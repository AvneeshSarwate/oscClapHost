use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, SampleFormat, SupportedStreamConfig};

pub struct DeviceInfo {
    pub index: u32,
    pub name: String,
    pub is_default: bool,
}

pub fn get_cpal_host() -> Host {
    cpal::default_host()
}

pub fn list_output_devices(host: &Host) -> Result<Vec<DeviceInfo>> {
    let default_device = host.default_output_device();
    let default_name = default_device
        .as_ref()
        .and_then(|d| d.name().ok());

    let devices: Vec<_> = host
        .output_devices()
        .context("Failed to enumerate output devices")?
        .enumerate()
        .filter_map(|(i, device)| {
            let name = device.name().ok()?;
            let is_default = default_name.as_ref().map(|dn| dn == &name).unwrap_or(false);
            Some(DeviceInfo {
                index: i as u32,
                name,
                is_default,
            })
        })
        .collect();

    Ok(devices)
}

pub fn print_devices(host: &Host) -> Result<()> {
    let devices = list_output_devices(host)?;
    
    if devices.is_empty() {
        println!("No output devices found.");
        return Ok(());
    }

    println!("Available audio output devices:");
    for device in &devices {
        let default_marker = if device.is_default { " (default)" } else { "" };
        println!("  [{}] {}{}", device.index, device.name, default_marker);
    }

    Ok(())
}

pub fn select_device(host: &Host, device_index: Option<u32>) -> Result<Device> {
    match device_index {
        Some(index) => {
            let devices: Vec<_> = host
                .output_devices()
                .context("Failed to enumerate output devices")?
                .collect();
            
            devices
                .into_iter()
                .nth(index as usize)
                .ok_or_else(|| anyhow!("Device index {} not found", index))
        }
        None => host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device available")),
    }
}

pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: u32,
    pub sample_format: SampleFormat,
}

pub fn get_device_config(
    device: &Device,
    preferred_sample_rate: Option<u32>,
    preferred_channels: Option<u16>,
    preferred_buffer_size: Option<u32>,
) -> Result<AudioConfig> {
    let default_config = device
        .default_output_config()
        .context("Failed to get default output config")?;

    let sample_rate = preferred_sample_rate.unwrap_or(default_config.sample_rate().0);
    let channels = preferred_channels.unwrap_or(default_config.channels());
    let buffer_size = preferred_buffer_size.unwrap_or(512);
    let sample_format = default_config.sample_format();

    Ok(AudioConfig {
        sample_rate,
        channels,
        buffer_size,
        sample_format,
    })
}

pub fn find_supported_config(
    device: &Device,
    config: &AudioConfig,
) -> Result<SupportedStreamConfig> {
    let supported_configs: Vec<_> = device
        .supported_output_configs()
        .context("Failed to get supported configs")?
        .collect();

    for cfg in &supported_configs {
        if cfg.channels() >= config.channels
            && cfg.min_sample_rate().0 <= config.sample_rate
            && cfg.max_sample_rate().0 >= config.sample_rate
        {
            return Ok(cfg.clone().with_sample_rate(cpal::SampleRate(config.sample_rate)));
        }
    }

    device
        .default_output_config()
        .context("No suitable output config found")
}
