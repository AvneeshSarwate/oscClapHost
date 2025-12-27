# OSC CLAP Host

A CLI CLAP audio plugin host that receives OSC messages for note and parameter control.

## Features

- Loads and runs any CLAP plugin from a `.clap` bundle
- Outputs audio to any system audio device via CPAL
- Listens for OSC messages on a configurable UDP port
- Supports:
  - Note on/off with explicit `note_id`
  - Global parameter value changes
  - Per-note parameter modulation (for plugins that support it)
- Prints OSC API and parameter table with `--print-osc`

## Building

```bash
cargo build --release
```

The binary will be at `target/release/clap-osc-host`.

## Usage

### List audio devices

```bash
clap-osc-host --list-devices
```

### List plugins in a bundle

```bash
clap-osc-host --list-plugins /path/to/plugin.clap
```

### Print OSC API and parameters

```bash
clap-osc-host --print-osc /path/to/plugin.clap
```

### Run the host

```bash
clap-osc-host /path/to/plugin.clap --osc-port 9000 --device 0
```

## OSC API

See text_per_note_mod.scd for a quick debug test using supercollider. Parameter ids for the SurgeXT synth are printed in surgeOSC.txt

### Note Control

| Address       | Arguments                                           | Description    |
|---------------|-----------------------------------------------------|----------------|
| `/note/on`    | `note_id:i32 key:i32 vel:f32 [chan:i32] [port:i32]` | Note on event  |
| `/note/off`   | `note_id:i32 key:i32 vel:f32 [chan:i32] [port:i32]` | Note off event |
| `/note/choke` | `note_id:i32 [key:i32] [chan:i32] [port:i32]`       | Note choke     |

### Parameter Control

| Address      | Arguments                                                        | Description              |
|--------------|------------------------------------------------------------------|--------------------------|
| `/param/set` | `param_id:i32 value:f64`                                         | Set global param value   |
| `/param/mod` | `note_id:i32 param_id:i32 amount:f64 [key:i32] [chan:i32] [port:i32]` | Per-note modulation |

**Note:** `/param/mod` only works for parameters that advertise `CLAP_PARAM_IS_MODULATABLE_PER_NOTE_ID`. Use `--print-osc` to see which parameters support per-note modulation.

## CLI Options

```
Usage: clap-osc-host [OPTIONS] [PLUGIN_PATH]

Arguments:
  [PLUGIN_PATH]  Path to the .clap plugin bundle

Options:
      --plugin-id <PLUGIN_ID>        Select plugin by CLAP descriptor id
      --plugin-index <PLUGIN_INDEX>  Select plugin by index
      --list-plugins                 Print plugin descriptors and exit
      --osc-port <OSC_PORT>          OSC UDP port [default: 9000]
  -p, --print-osc                    Print OSC API and parameter table, then exit
      --list-devices                 Print available audio output devices and exit
      --device <DEVICE>              Audio output device index
      --sample-rate <SAMPLE_RATE>    Sample rate
      --buffer-size <BUFFER_SIZE>    Buffer size in frames
      --channels <CHANNELS>          Number of output channels
  -h, --help                         Print help
```

## Example with oscsend

```bash
# Start the host
clap-osc-host /Library/Audio/Plug-Ins/CLAP/Surge\ XT.clap --osc-port 9000

# In another terminal, send OSC messages
oscsend localhost 9000 /note/on iii 1 60 0.8    # note_id=1, key=60 (C4), vel=0.8
oscsend localhost 9000 /param/set if 0 0.5      # set param 0 to 0.5
oscsend localhost 9000 /note/off iii 1 60 0.0   # note off
```

## Dependencies

- [Clack](https://github.com/prokopyl/clack) - CLAP hosting in Rust
- [CPAL](https://github.com/RustAudio/cpal) - Cross-platform audio I/O
- [rosc](https://github.com/klingtnet/rosc) - OSC protocol implementation



## TODO 
- [ ] save/load presets