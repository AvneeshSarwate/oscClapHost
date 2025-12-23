use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "clap-osc-host")]
#[command(about = "A CLI CLAP host that receives OSC messages for note and parameter control")]
pub struct Args {
    /// Path to the .clap plugin bundle
    #[arg(required_unless_present_any = ["list_devices"])]
    pub plugin_path: Option<PathBuf>,

    /// Select plugin by CLAP descriptor id (if bundle contains multiple plugins)
    #[arg(long = "plugin-id")]
    pub plugin_id: Option<String>,

    /// Select plugin by index as listed by --list-plugins
    #[arg(long = "plugin-index")]
    pub plugin_index: Option<u32>,

    /// Print plugin descriptors (index, id, name) and exit
    #[arg(long = "list-plugins")]
    pub list_plugins: bool,

    /// OSC UDP port to listen on (default: 9000)
    #[arg(long = "osc-port", default_value = "9000")]
    pub osc_port: u16,

    /// Print the OSC API and parameter table, then exit
    #[arg(short = 'p', long = "print-osc")]
    pub print_osc: bool,

    /// Print available audio output devices and exit
    #[arg(long = "list-devices")]
    pub list_devices: bool,

    /// Audio output device index (default: system default)
    #[arg(long = "device")]
    pub device: Option<u32>,

    /// Sample rate (default: device's preferred rate)
    #[arg(long = "sample-rate")]
    pub sample_rate: Option<u32>,

    /// Buffer size in frames (default: device's default)
    #[arg(long = "buffer-size")]
    pub buffer_size: Option<u32>,

    /// Number of output channels (default: match plugin output, usually 2)
    #[arg(long = "channels")]
    pub channels: Option<u16>,
}
