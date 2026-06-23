use pulsectl::controllers::{DeviceControl, SinkController};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::data_type::DataType;

use super::super::_base::Provider;

fn get_volume() -> Option<f32> {
    get_wpctl_volume().or_else(get_pulse_volume)
}

fn get_wpctl_volume() -> Option<f32> {
    let output = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.split_whitespace().find_map(|part| part.parse::<f32>().ok())
}

fn get_pulse_volume() -> Option<f32> {
    let mut controller = SinkController::create().ok()?;
    if let Ok(default) = controller.get_default_device() {
        let device_volume = default.volume.get().first()?.0 as f32;
        let base_volume = default.base_volume.0 as f32;
        return Some(device_volume / base_volume);
    }

    return None;
}

fn send_data(value: &f32, push_sender: &broadcast::Sender<Vec<u8>>) {
    let volume = (value * 100.0).round().clamp(0.0, 255.0) as u8;
    let data = vec![DataType::Volume as u8, volume];
    push_sender.send(data).unwrap();
}

pub struct VolumeProvider {
    data_sender: broadcast::Sender<Vec<u8>>,
    is_started: Arc<AtomicBool>,
}

impl VolumeProvider {
    pub fn new(data_sender: broadcast::Sender<Vec<u8>>) -> Box<dyn Provider> {
        let provider = VolumeProvider {
            data_sender,
            is_started: Arc::new(AtomicBool::new(false)),
        };
        return Box::new(provider);
    }
}

impl Provider for VolumeProvider {
    fn start(&self) {
        tracing::info!("Volume Provider started");
        self.is_started.store(true, Relaxed);
        let data_sender = self.data_sender.clone();
        let is_started = self.is_started.clone();

        std::thread::spawn(move || {
            let mut volume = None;

            loop {
                if !is_started.load(Relaxed) {
                    break;
                }

                let new_volume = get_volume().unwrap_or_default();
                if volume != Some(new_volume) {
                    volume = Some(new_volume);
                    send_data(&new_volume, &data_sender);
                }

                std::thread::sleep(Duration::from_millis(250));
            }

            tracing::info!("Volume Provider stopped");
        });
    }

    fn stop(&self) {
        self.is_started.store(false, Relaxed);
    }
}
