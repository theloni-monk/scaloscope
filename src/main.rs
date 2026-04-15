// src/main.rs

use cpal::{
    Device, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange, traits::{DeviceTrait, HostTrait}
};

// use std::error::Error;

use nih_plug::prelude::*;
use std::{
    error::Error,
    io::{self, Write},
};

use scaloscope::Scaloscope;

// written by a clanker, may not be trustworthy, use at your own risk
fn wrapper_args_from_cpal(
    idevice: &Device,
    iconfig: &SupportedStreamConfig,
    odevice: &Device,
    oconfig: &SupportedStreamConfig,
) -> Vec<String> {
    let mut args = vec!["scaloscope".to_string()];
    let period_size = match iconfig.buffer_size() {
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
        iconfig.sample_rate().0.to_string(),
        "--period-size".to_string(),
        period_size.to_string(),
    ]);

    if let Ok(device_name) = idevice.name() {
        args.extend(["--input-device".to_string(), device_name]);
    }

    if let Ok(device_name) = odevice.name() {
        args.extend(["--output-device".to_string(), device_name]);
    }

    args
}

fn supports_input(device: &Device) -> bool {
    device
        .supported_input_configs()
        .is_ok_and(|mut iter| iter.next().is_some())
}

fn prompt_device_and_config(
    devices: Vec<Device>,
    default_device: Device,
    device_type: &str,
    is_input: bool,
) -> Result<(Device, cpal::SupportedStreamConfig), Box<dyn Error>> {
    println!("\nAvailable {} devices:", device_type);
    for (i, d) in devices.iter().enumerate() {
        let device_name = d.name().unwrap_or("<Unknown>".to_string());

        #[cfg(target_os = "windows")]
        {
            if is_input {
                let loopback_hint = if supports_input(d) {
                    ""
                } else {
                    " (WASAPI loopback candidate: no input configs detected)"
                };
                println!("  [{}] {}{}", i, device_name, loopback_hint);
            } else {
                println!("  [{}] {}", i, device_name);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            println!("  [{}] {}", i, device_name);
        }
    }

    let mut selected_device: Device = default_device;
    loop {
        // prompt user to select device (press Enter to choose default)
        print!("Select {} device index (press Enter for default device): ", device_type);
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
            selected_device = match devices.get(idx) {
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
        "Selected {} device: {}",
        device_type,
        selected_device.name().unwrap_or("<Unknown>".to_string())
    );

    // list supported configs for chosen device
    let configs: Vec<SupportedStreamConfigRange> = if is_input {
        selected_device.supported_input_configs()?.into_iter().collect()
    } else {
        selected_device.supported_output_configs()?.into_iter().collect()
    };

    println!(
        "Supported {} configs for '{}':",
        device_type,
        selected_device.name().unwrap_or("<Unknown>".to_string())
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
        print!("Select {} config index (press Enter for first config) [note: currently only f32 is supported]: ", device_type);
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

    Ok((selected_device, supported_config))
}

fn select_device_and_config() -> Result<((Device, cpal::SupportedStreamConfig),
                                        (Device, cpal::SupportedStreamConfig)), Box<dyn Error>> {
    // setup audio stream - interactive device & config selection
    let host = cpal::default_host();

    // gather input devices
    let input_devices = host
        .input_devices()
        .expect("No input devices available")
        .into_iter()
        .collect::<Vec<Device>>();
    let default_input = host
        .default_input_device()
        .expect("No default input device available");

    let input_selection = prompt_device_and_config(input_devices, default_input, "input", true)?;

    // gather output devices
    let output_devices = host
        .output_devices()
        .expect("No output devices available")
        .into_iter()
        .collect::<Vec<Device>>();
    let default_output = host
        .default_output_device()
        .expect("No default output device available");

    let output_selection = prompt_device_and_config(output_devices, default_output, "output", false)?;

    Ok((input_selection, output_selection))
}



fn main() -> Result<(), Box<dyn Error>> {
    let ((idevice, iconfig), (odevice, oconfig)) = select_device_and_config()?;
    let args = wrapper_args_from_cpal(&idevice, &iconfig, &odevice, &oconfig);
    println!("passing args: {:?}", args.join("; "));
    let _ok = nih_export_standalone_with_args::<Scaloscope, _>(args);
    Ok(())
}
