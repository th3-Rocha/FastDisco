//! The microphones and speakers to choose from, by the names the system
//! shows, and putting FastDisco's sound on the chosen ones.
//!
//! On Linux the sound server (PipeWire or PulseAudio) knows the devices
//! by name, "UGREEN HiTune Max5c" rather than ALSA's "pipewire"; FastDisco
//! opens the default device and asks `pactl` to move its own streams,
//! as pavucontrol does. Elsewhere the default device is used.

/// A device: the system's name for it and the one people read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Devices {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
    /// The system's defaults, for "Padrão do sistema: …".
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

impl Devices {
    pub fn label(list: &[Device], name: &str) -> Option<String> {
        list.iter()
            .find(|d| d.name == name)
            .map(|d| d.label.clone())
    }
}

#[cfg(target_os = "linux")]
fn pactl(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("pactl")
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(target_os = "linux")]
fn list_of(kind: &str) -> Vec<serde_json::Value> {
    pactl(&["-f", "json", "list", kind])
        .and_then(|text| serde_json::from_str::<Vec<serde_json::Value>>(&text).ok())
        .unwrap_or_default()
}

/// What the system offers now; slow enough to keep off the interface.
pub fn list() -> Devices {
    #[cfg(target_os = "linux")]
    {
        let devices = |kind: &str| -> Vec<Device> {
            list_of(kind)
                .iter()
                // A monitor is what another device plays, not a microphone.
                .filter(|d| d["monitor_of_sink"].is_null() || kind == "sinks")
                .filter_map(|d| {
                    Some(Device {
                        name: d["name"].as_str()?.to_owned(),
                        label: d["description"].as_str()?.to_owned(),
                    })
                })
                .filter(|d| !d.name.ends_with(".monitor"))
                .collect()
        };
        Devices {
            inputs: devices("sources"),
            outputs: devices("sinks"),
            default_input: pactl(&["get-default-source"]),
            default_output: pactl(&["get-default-sink"]),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Devices::default()
    }
}

/// Moves FastDisco's playing and recording streams to the chosen devices,
/// or back to the defaults; in the background, after the streams appear.
pub fn route(input: Option<String>, output: Option<String>) {
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        // A stream just opened takes a moment to show up.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let pid = std::process::id().to_string();
        // PipeWire's ALSA streams carry no process id, only the program's
        // name: alsa_playback.fastdisco, alsa_capture.fastdisco.
        let program = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "fastdisco".into());
        let ours = |kind: &str| -> Vec<String> {
            list_of(kind)
                .iter()
                .filter(|s| {
                    let props = &s["properties"];
                    props["application.process.id"].as_str() == Some(pid.as_str())
                        || props["node.name"]
                            .as_str()
                            .and_then(|n| n.split_once('.'))
                            .is_some_and(|(kind, name)| {
                                kind.starts_with("alsa_") && name == program
                            })
                })
                .filter_map(|s| s["index"].as_u64().map(|i| i.to_string()))
                .collect()
        };
        let output = output.unwrap_or_else(|| "@DEFAULT_SINK@".into());
        for index in ours("sink-inputs") {
            let _ = pactl(&["move-sink-input", &index, &output]);
        }
        let input = input.unwrap_or_else(|| "@DEFAULT_SOURCE@".into());
        for index in ours("source-outputs") {
            let _ = pactl(&["move-source-output", &index, &input]);
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = (input, output);
}
