Below is an implementation plan you can hand to a coding agent for a **CLI CLAP host (Rust + Clack)** that:

* runs a CLAP plugin (e.g., Surge XT) from a **plugin path**
* outputs audio to a selected **audio device** (by index)
* listens on a **localhost OSC UDP port**
* accepts OSC for **note on/off**, **global parameter set**, and **per-note parameter modulation**
* has `-p/--print-osc` to print the full OSC API + a **param table** (including which params support per-note modulation)

This plan uses:

* **Clack** (`clack-host`, `clack-extensions`) for CLAP hosting ([GitHub][1])
* **CPAL** for cross-platform audio device enumeration + output streams ([Docs.rs][2])
* **rosc** for OSC decode/encode ([Docs.rs][3])
* **clap** (Rust crate) for command-line argument parsing ([Docs.rs][4])

---

## 0) Key design constraints (what makes this “work”)

### Per-note modulation in CLAP is not “anything, always”

You must only send per-note modulation for parameters that advertise the CLAP flag:

* `CLAP_PARAM_IS_MODULATABLE_PER_NOTE_ID` ([GitHub][5])

So: enumerate params at startup, cache which params have that flag, and reject `/param/mod` for params that don’t.

### Audio output & device listing

CPAL gives you:

* `default_host()`
* enumerate devices
* build output stream callbacks ([Docs.rs][2])
  It also ships an `enumerate` example that lists devices/capabilities (useful reference). ([GitHub][6])

### CLAP hosting scaffolding

Clack’s readme includes a working `clack-host` example (bundle load → pick descriptor → activate → process). ([GitHub][1])
Use that as the skeleton; then replace the “process a couple samples” section with “process buffers from CPAL callback”.

---

## 1) CLI UX spec (what the binary supports)

Binary name suggestion: `clap-osc-host`

### Basic usage

```bash
clap-osc-host /path/to/plugin.clap \
  --osc-port 9000 \
  --device 3
```

### Arguments

Use `clap` derive macros. ([Docs.rs][4])

**Positional**

* `PLUGIN_PATH` (PathBuf): path to the `.clap` bundle/shared library

**Selection**

* `--plugin-id <string>` (optional): if bundle contains multiple plugins, select by CLAP descriptor id
* `--plugin-index <u32>` (optional): select plugin by index as listed by `--list-plugins`
* `--list-plugins` (flag): prints descriptors (index, id, name) then exits

**OSC**

* `--osc-port <u16>` default `9000` (bind `127.0.0.1:port`)
* `-p, --print-osc` (flag): prints the OSC API + parameter table for loaded plugin, then exits

**Audio**

* `--list-devices` (flag): prints output devices with indices + default marker, then exits
* `--device <u32>` (optional): output device index; default = system default output
* `--sample-rate <u32>` (optional): default use device’s preferred/max supported
* `--buffer-size <u32>` (optional): attempt to request `BufferSize::Fixed(n)`; if unsupported, fall back to default
* `--channels <u16>` (optional): default match plugin main output (usually 2)

---

## 2) OSC API spec (what messages it accepts)

Use **one stable “address family”** so your external generator can be simple.

All messages are UDP OSC to `127.0.0.1:<osc-port>`. Decode with `rosc`. ([Docs.rs][3])

### Required: note on/off with explicit `note_id`

CLAP note events include `note_id`, `key`, `channel`, etc. Clack’s host example shows using `NoteOnEvent` and a “Pckn” (port/channel/key/note-id) style value. ([GitHub][1])

**`/note/on`**

* args: `note_id:i32 key:i32 vel:f32 [chan:i32=0] [port:i32=0]`
* mapping: emit CLAP NoteOn event with those fields

**`/note/off`**

* args: `note_id:i32 key:i32 vel:f32 [chan:i32=0] [port:i32=0]`
* mapping: emit CLAP NoteOff event

**`/note/choke`** (optional but useful)

* args: `note_id:i32 [key:i32=-1] [chan:i32=-1] [port:i32=-1]`
* mapping: emit CLAP NoteChoke event (if you wire it)

### Global parameter set (base value)

**`/param/set`**

* args: `param_id:u32 value:f64`
* mapping: emit CLAP ParamValue event with `note_id=-1`, `key=-1`, `channel=-1`, `port=-1` (global)

### Per-note modulation (temporary offset)

CLAP supports non-destructive “modulation amount” separate from “base value”. This is the feature you want.

**`/param/mod`**

* args: `note_id:i32 param_id:u32 amount:f64 [key:i32=-1] [chan:i32=-1] [port:i32=-1]`
* mapping: emit CLAP ParamMod event with note scoping.
* validation: only allow if param’s flags include `CLAP_PARAM_IS_MODULATABLE_PER_NOTE_ID`. ([GitHub][5])

### Print-OSC output (`-p/--print-osc`)

This should print:

1. the fixed OSC endpoints above, with argument types
2. a parameter table:

   * `param_id`, `name`, `module/path`, `min`, `max`, `default`
   * flags: `modulatable`, `modulatable_per_note_id`, etc. (at least those)
   * and ideally: “per-note modulatable: YES/NO” derived from the flag ([GitHub][5])

---

## 3) Crates / dependencies (Cargo.toml)

Minimum set:

* `clack-host`, `clack-extensions` (git dependency if not on crates.io in your agent’s environment; Clack is the CLAP host framework) ([GitHub][1])
* `cpal = "0.17"` (audio I/O; can enumerate devices and build output streams) ([Docs.rs][2])

  * consider features: `jack` optional on Linux if you want JACK support (CPAL lists `jack` as optional) ([Docs.rs][2])
* `rosc` (OSC decode/encode) ([Docs.rs][3])
* `clap = { version = "4", features=["derive"] }` (CLI args parsing) ([Docs.rs][4])
* `anyhow` (error handling)
* `log` + `env_logger` (or `tracing` + `tracing-subscriber`)
* **Realtime-safe queue**: `crossbeam_queue` (bounded `ArrayQueue`) or `ringbuf` (SPSC)

  * OSC thread pushes commands into queue
  * audio callback pops without allocation

Optional nice-to-haves:

* `audio_thread_priority` (Linux tuning; CPAL mentions it as optional dep) ([Docs.rs][2])
* `serde` + `serde_json` (if you want `--dump-params-json`)

---

## 4) High-level architecture

### Threads

1. **Main thread**

   * parse args
   * load plugin (Clack)
   * enumerate params (Clack params extension)
   * set up shared event queue
   * start OSC receiver thread
   * start CPAL audio stream (which runs callback on a dedicated audio thread) ([Docs.rs][2])

2. **OSC thread**

   * `UdpSocket::bind(("127.0.0.1", osc_port))`
   * `recv_from` loop
   * decode bytes with `rosc::decoder`
   * match addresses and push “Command” structs to queue

3. **Audio callback thread** (CPAL)

   * pop all pending commands (non-blocking)
   * translate into CLAP input events for this buffer (time=0 for “asap”)
   * call plugin `process(...)`
   * copy plugin output into CPAL output buffer

### Internal modules (suggested file layout)

* `src/main.rs` – orchestration
* `src/args.rs` – clap derive struct
* `src/device.rs` – list devices; select by index
* `src/plugin.rs` – clack bundle load; descriptor selection; activation; param enumeration; event builders
* `src/osc.rs` – UDP receive + decode + command parsing
* `src/engine.rs` – audio callback glue (queue → CLAP events → audio buffer)

---

## 5) Detailed implementation steps (milestones)

### Milestone A — Device listing & selection

Implement `--list-devices` using CPAL:

* `let host = cpal::default_host();`
* enumerate output devices and print:

  * index
  * `device.name()`
  * mark default output device
* exit

CPAL docs describe host/device/stream concepts and enumerating devices. ([Docs.rs][2])
Also CPAL repo includes an `enumerate` example. ([GitHub][6])

### Milestone B — Load CLAP plugin and pick descriptor

Using Clack:

* `PluginBundle::load(plugin_path)`
* `get_plugin_factory()`
* list descriptors if `--list-plugins`
* choose plugin by:

  * `--plugin-id`, else `--plugin-index`, else if only one plugin then auto
* create `PluginInstance`
* implement minimal `HostHandlers` (stubs for request_restart/process/callback)
* activate with `PluginAudioConfiguration`:

  * sample_rate (from CPAL config)
  * min/max frames (based on buffer size strategy)

Clack’s README shows this flow and a NoteOnEvent example. ([GitHub][1])

### Milestone C — Parameter enumeration + cache

On main thread, query `clap.params` extension via Clack:

* for each param index:

  * get_info → id, name, min/max/default, flags
  * cache in `Vec<ParamInfo>`
* derive:

  * `is_modulatable_per_note_id = flags & CLAP_PARAM_IS_MODULATABLE_PER_NOTE_ID != 0` ([GitHub][5])

Use this cache for:

* validating `/param/mod`
* printing `--print-osc`

### Milestone D — Implement `--print-osc`

`--print-osc` should:

* print OSC endpoints + arg types
* print param table
* exit

### Milestone E — OSC receiver

Implement OSC parsing with `rosc`:

* decode incoming datagram to `OscPacket`
* if message:

  * switch on `addr` (e.g., `/note/on`)
  * parse args by type (i32/f32/f64)
  * build an internal `Command` enum:

    * `NoteOn { note_id, key, vel, chan, port }`
    * `NoteOff { ... }`
    * `ParamSet { param_id, value }`
    * `ParamMod { note_id, param_id, amount, key?, chan?, port? }`
* push into bounded queue

rosc provides encoder/decoder + `OscMessage` struct with `addr` and `args`. ([Docs.rs][3])

### Milestone F — Audio engine glue (CPAL callback)

Use CPAL to build an output stream:

* choose device by index or default
* choose output config (prefer f32)
* create `build_output_stream(...)` callback ([Docs.rs][2])

Inside callback:

1. Determine `frames = data.len() / channels`
2. Drain command queue into a local stack vec (or fixed array)
3. Convert commands → CLAP input event buffer (time=0 for now)
4. Prepare Clack `AudioPorts` buffers:

   * plugin output buffers (f32)
5. Call plugin processor `.process(...)`
6. Convert plugin output f32 → CPAL sample format (if needed) and interleave into `data`

CPAL docs include sample-format branching patterns (F32/I16/U16) and stream callback behavior. ([Docs.rs][2])

### Milestone G — Validation & error strategy

* If OSC sends `/param/mod` for a param not per-note modulatable → log error (or ignore)
* If note_id not currently active → still allow (some plugins may ignore; or treat as future note)
* If device channel count doesn’t match plugin outputs:

  * simplest: require output channels >= plugin channels; else fail fast with explanation

---

## 6) Testing plan (simple + fast)

1. Run `--list-devices`, pick a device index.
2. Run `--list-plugins` for the bundle (if needed).
3. Run `--print-osc` to see param list and which accept per-note mod.
4. Start the host; use `oscsend` to send:

   * `/note/on` with a note_id you track
   * `/param/mod` for that note_id on a per-note modulatable param
   * `/note/off`

(Agent can add an “OSC smoke test” script in Python later, but not required for MVP.)

---

## 7) Notes for Raspberry Pi deployment (hand-off guidance)

* Cross-compiling the **host** from macOS is feasible, but the **plugin binary** must be built for **aarch64 Linux** separately (Surge CLAP on Pi must be ARM64).
* CPAL on Linux may require system audio libs (ALSA/Pulse/JACK). If you want JACK specifically, enable CPAL’s optional JACK backend. ([Docs.rs][2])

---

If you want, I can also provide a concise “agent ticket” version (checklist + acceptance criteria) you can paste directly into your coding agent, but the above is already structured for handoff.

[1]: https://github.com/prokopyl/clack "GitHub - prokopyl/clack: Safe, low-level wrapper to create CLAP audio plugins and hosts in Rust"
[2]: https://docs.rs/cpal "cpal - Rust"
[3]: https://docs.rs/rosc "rosc - Rust"
[4]: https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html?utm_source=chatgpt.com "clap::_derive::_tutorial - Rust"
[5]: https://raw.githubusercontent.com/free-audio/clap/main/include/clap/ext/params.h "raw.githubusercontent.com"
[6]: https://github.com/RustAudio/cpal?utm_source=chatgpt.com "RustAudio/cpal: Cross-platform audio I/O library in pure Rust"
