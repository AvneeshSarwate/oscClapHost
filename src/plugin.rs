use anyhow::{Context, Result, anyhow};
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_host::prelude::*;
use std::ffi::CString;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PluginDescriptorInfo {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub vendor: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
    pub is_modulatable: bool,
    pub is_modulatable_per_note_id: bool,
    pub is_automatable: bool,
    pub is_stepped: bool,
}

pub fn load_bundle(path: &Path) -> Result<PluginBundle> {
    unsafe { PluginBundle::load(path) }.context("Failed to load CLAP plugin bundle")
}

pub fn list_plugins_in_bundle(bundle: &PluginBundle) -> Result<Vec<PluginDescriptorInfo>> {
    let factory = bundle
        .get_plugin_factory()
        .ok_or_else(|| anyhow!("Bundle has no plugin factory"))?;

    let descriptors: Vec<_> = factory
        .plugin_descriptors()
        .enumerate()
        .filter_map(|(i, desc)| {
            let id = desc.id()?.to_str().ok()?.to_string();
            let name = desc.name()?.to_str().ok()?.to_string();
            let vendor = desc.vendor().and_then(|v| v.to_str().ok().map(String::from));
            let version = desc.version().and_then(|v| v.to_str().ok().map(String::from));
            Some(PluginDescriptorInfo {
                index: i,
                id,
                name,
                vendor,
                version,
            })
        })
        .collect();

    Ok(descriptors)
}

pub fn print_plugins(bundle: &PluginBundle) -> Result<()> {
    let plugins = list_plugins_in_bundle(bundle)?;

    if plugins.is_empty() {
        println!("No plugins found in bundle.");
        return Ok(());
    }

    println!("Plugins in bundle:");
    for plugin in &plugins {
        let vendor = plugin.vendor.as_deref().unwrap_or("unknown");
        let version = plugin.version.as_deref().unwrap_or("?");
        println!(
            "  [{}] {} - {} (vendor: {}, version: {})",
            plugin.index, plugin.id, plugin.name, vendor, version
        );
    }

    Ok(())
}

pub fn select_plugin_id(
    bundle: &PluginBundle,
    plugin_id: Option<&str>,
    plugin_index: Option<u32>,
) -> Result<CString> {
    let plugins = list_plugins_in_bundle(bundle)?;

    if plugins.is_empty() {
        return Err(anyhow!("No plugins found in bundle"));
    }

    let selected = if let Some(id) = plugin_id {
        plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("Plugin with id '{}' not found", id))?
    } else if let Some(index) = plugin_index {
        plugins
            .get(index as usize)
            .ok_or_else(|| anyhow!("Plugin index {} out of range", index))?
    } else if plugins.len() == 1 {
        &plugins[0]
    } else {
        return Err(anyhow!(
            "Multiple plugins in bundle. Use --plugin-id or --plugin-index to select one."
        ));
    };

    CString::new(selected.id.as_str()).context("Invalid plugin ID string")
}

pub fn enumerate_params<H: HostHandlers>(
    instance: &mut PluginInstance<H>,
) -> Vec<ParamInfo> {
    let params_ext: Option<PluginParams> = instance.plugin_handle().get_extension();

    let Some(params_ext) = params_ext else {
        return Vec::new();
    };

    let mut handle = instance.plugin_handle();
    let count = params_ext.count(&mut handle);
    let mut result = Vec::with_capacity(count as usize);
    let mut buffer = ParamInfoBuffer::new();

    for i in 0..count {
        if let Some(info) = params_ext.get_info(&mut handle, i, &mut buffer) {
            let name = String::from_utf8_lossy(info.name).trim_end_matches('\0').to_string();
            let module = String::from_utf8_lossy(info.module).trim_end_matches('\0').to_string();

            result.push(ParamInfo {
                id: info.id.get(),
                name,
                module,
                min_value: info.min_value,
                max_value: info.max_value,
                default_value: info.default_value,
                is_modulatable: info.flags.contains(ParamInfoFlags::IS_MODULATABLE),
                is_modulatable_per_note_id: info
                    .flags
                    .contains(ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID),
                is_automatable: info.flags.contains(ParamInfoFlags::IS_AUTOMATABLE),
                is_stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
            });
        }
    }

    result
}

pub fn print_osc_api(params: &[ParamInfo]) {
    println!("=== OSC API ===\n");

    println!("Note Control:");
    println!("  /note/on     note_id:i32  key:i32  vel:f32  [chan:i32=0]  [port:i32=0]");
    println!("  /note/off    note_id:i32  key:i32  vel:f32  [chan:i32=0]  [port:i32=0]");
    println!("  /note/choke  note_id:i32  [key:i32=-1]  [chan:i32=-1]  [port:i32=-1]");
    println!();

    println!("Parameter Control:");
    println!("  /param/set   param_id:i32  value:f64");
    println!("  /param/mod   note_id:i32  param_id:i32  amount:f64  [key:i32=-1]  [chan:i32=-1]  [port:i32=-1]");
    println!();

    println!("=== Parameter Table ===\n");
    println!(
        "{:>8}  {:40}  {:30}  {:>12}  {:>12}  {:>12}  {:>8}  {:>12}",
        "ID", "Name", "Module", "Min", "Max", "Default", "Stepped", "Per-Note Mod"
    );
    println!("{}", "-".repeat(150));

    for param in params {
        let per_note = if param.is_modulatable_per_note_id {
            "YES"
        } else {
            "NO"
        };
        let stepped = if param.is_stepped { "YES" } else { "NO" };

        println!(
            "{:>8}  {:40}  {:30}  {:>12.4}  {:>12.4}  {:>12.4}  {:>8}  {:>12}",
            param.id,
            truncate(&param.name, 40),
            truncate(&param.module, 30),
            param.min_value,
            param.max_value,
            param.default_value,
            stepped,
            per_note,
        );
    }

    println!();
    println!("Total parameters: {}", params.len());
    let per_note_count = params.iter().filter(|p| p.is_modulatable_per_note_id).count();
    println!("Per-note modulatable: {}", per_note_count);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
