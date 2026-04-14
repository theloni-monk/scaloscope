// src/main.rs
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use nih_plug::prelude::*;
use std::{
    error::Error,
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use scaloscope::Scaloscope;

// written by a clanker, may not be trustworthy, use at your own risk
fn wrapper_args_from_cpal(
    device: &Device,
    supported_config: &SupportedStreamConfig,
) -> Vec<String> {
    let mut args = vec!["scaloscope".to_string()];
    let period_size = match supported_config.buffer_size() {
        SupportedBufferSize::Range { min, .. } => *min,
        SupportedBufferSize::Unknown => 512,
    };

    // Pin the backend to the current platform's CPAL host family.
    #[cfg(target_os = "windows")]
    {
        args.extend(["--backend".to_string(), "wasapi".to_string()]);
    }
    #[cfg(target_os = "macos")]
    {
        args.extend(["--backend".to_string(), "core-audio".to_string()]);
    }
    #[cfg(target_os = "linux")]
    {
        args.extend(["--backend".to_string(), "alsa".to_string()]);
    }

    args.extend([
        "--sample-rate".to_string(),
        supported_config.sample_rate().0.to_string(),
        "--period-size".to_string(),
        period_size.to_string(),
    ]);

    if let Ok(device_name) = device.name() {
        args.extend(["--input-device".to_string(), device_name]);
    }

    args
}

fn supports_input(device: &Device) -> bool {
    device
        .supported_input_configs()
        .is_ok_and(|mut iter| iter.next().is_some())
}

fn select_device_and_config() -> Result<(Device, cpal::SupportedStreamConfig), Box<dyn Error>> {
    // setup audio stream - interactive device & config selection
    let host = cpal::default_host();

    // gather input devices
    let devices = host
        .input_devices()
        .expect("No input devices available")
        .into_iter()
        .collect::<Vec<Device>>();

    println!("Available input devices:");
    for (i, d) in devices.iter().enumerate() {
        let device_name = d.name().unwrap_or("<Unknown>".to_string());

        #[cfg(target_os = "windows")]
        {
            let loopback_hint = if supports_input(d) {
                ""
            } else {
                " (WASAPI loopback candidate: no input configs detected)"
            };
            println!("  [{}] {}{}", i, device_name, loopback_hint);
        }

        #[cfg(not(target_os = "windows"))]
        {
            println!("  [{}] {}", i, device_name);
        }
    }

    let mut device: Device = host
        .default_input_device()
        .expect("No default input device available");
    loop {
        // prompt user to select device (press Enter to choose default)
        print!("Select device index (press Enter for default device): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let selection = input.trim();

        if !selection.is_empty() {
            let idx: usize = match selection.parse() {
                Ok(idx) => idx,
                Err(_) => {
                    println!("Invalid Selection");
                    continue;
                }
            };
            device = match devices.get(idx) {
                Some(device) => device.clone(),
                None => {
                    println!("Invalid Selection");
                    continue;
                }
            };
        };
        break;
    }

    println!(
        "Selected device: {}",
        device.name().unwrap_or("<Unknown>".to_string())
    );
    // list supported configs for chosen device
    let configs = device
        .supported_input_configs()
        .expect("error while querying configs")
        .into_iter()
        .collect::<Vec<SupportedStreamConfigRange>>();

    println!(
        "Supported configs for '{}':",
        device.name().unwrap_or("<Unknown>".to_string())
    );
    for (i, c) in configs.iter().enumerate() {
        println!(
            "  [{}] {:?}, channels: {}, min_rate: {}, max_rate: {}, buffer_size: {:?}",
            i,
            c.sample_format(),
            c.channels(),
            c.min_sample_rate().0,
            c.max_sample_rate().0,
            c.buffer_size()
        );
    }

    // prompt user to select config (press Enter to choose first config)
    let idx: usize = loop {
        print!("Select config index (press Enter for first config) [note: currently only f32 is supported]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        input.clear();
        io::stdin().read_line(&mut input)?;
        let selection = input.trim();

        if selection.is_empty() {
            break 0;
        } else {
            match selection.parse::<usize>() {
                Ok(idx) => {
                    if idx < configs.len() {
                        break idx;
                    }
                    println!("Invalid Selection");
                    continue;
                }
                Err(_) => {
                    println!("Invalid Selection");
                    continue;
                }
            }
        }
    };

    let supported_config = configs
        .into_iter()
        .nth(idx)
        .expect("Invalid Stream Config")
        .with_max_sample_rate();
    Ok((device, supported_config))
}



fn main() -> Result<(), Box<dyn Error>> {
    let (device, supported_config) = select_device_and_config()?;
    let mut args = wrapper_args_from_cpal(&device, &supported_config);
    println!("Passing args to nih_export_standalone_with_args: {:?}", args.join("; "));
    args.extend(["--output-device".to_string(), "".to_string()]);
    let _ok = nih_export_standalone_with_args::<Scaloscope, _>(args);
    Ok(())
}
