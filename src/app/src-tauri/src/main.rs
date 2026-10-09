#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use mic_engine::{
    audio::{self, Controller, DeviceInfo, Status},
    dsp::FilterConfig,
    settings::{Loaded, Session, Store},
};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Manager, State};
struct NativeState {
    audio: Mutex<Controller>,
    store: Mutex<Store>,
}
fn poisoned() -> String {
    "应用状态不可用，请重新启动".into()
}
#[tauri::command]
async fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    audio::list_devices()
}
#[tauri::command]
async fn load_session(state: State<'_, NativeState>) -> Result<Loaded, String> {
    state.store.lock().map_err(|_| poisoned())?.load()
}
#[tauri::command]
async fn save_session(state: State<'_, NativeState>, session: Session) -> Result<(), String> {
    state.store.lock().map_err(|_| poisoned())?.save(&session)
}
#[tauri::command]
async fn set_chain(
    state: State<'_, NativeState>,
    filters: Vec<FilterConfig>,
) -> Result<(), String> {
    state
        .audio
        .lock()
        .map_err(|_| poisoned())?
        .set_chain(&filters)
}
#[tauri::command]
async fn start_audio(state: State<'_, NativeState>, settings: Session) -> Result<(), String> {
    state.audio.lock().map_err(|_| poisoned())?.start(&settings)
}
#[tauri::command]
async fn stop_audio(state: State<'_, NativeState>) -> Result<(), String> {
    state.audio.lock().map_err(|_| poisoned())?.stop();
    Ok(())
}
#[tauri::command]
async fn set_monitoring(state: State<'_, NativeState>, enabled: bool) -> Result<(), String> {
    state
        .audio
        .lock()
        .map_err(|_| poisoned())?
        .set_monitoring(enabled)
}
#[tauri::command]
async fn get_status(state: State<'_, NativeState>) -> Result<Status, String> {
    Ok(state.audio.lock().map_err(|_| poisoned())?.status())
}
#[tauri::command]
async fn install_cable(app: tauri::AppHandle) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("resources/vb-cable/VBCABLE_Setup_x64.exe");
    if !path.is_file() {
        return Err("随包附带的驱动安装程序缺失，请从 VB-Audio 官方网站获取".into());
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; Start-Process -FilePath '{}' -Verb RunAs",
        path.to_string_lossy().replace('\'', "''")
    );
    let result = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err("安装未启动或被取消；请允许管理员提示后再试".into());
    }
    Ok(
        "已打开 VB-CABLE 官方安装程序。完成安装后重启 Windows，再选择 CABLE Input / CABLE Output。"
            .into(),
    )
}
#[tauri::command]
async fn open_cable_site() -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg("https://vb-audio.com/Cable/")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn directory(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let parent = executable.parent().ok_or("Executable has no directory")?;
    if parent.join("portable.marker").is_file() {
        Ok(parent.join("data"))
    } else {
        Ok(app.path().app_data_dir()?)
    }
}
fn diagnostic() -> Option<i32> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--devices") {
        match audio::list_devices() {
            Ok(devices) => {
                println!("{}", serde_json::to_string_pretty(&devices).unwrap());
                return Some(0);
            }
            Err(error) => {
                eprintln!("{error}");
                return Some(1);
            }
        }
    }
    if let Some(index) = args.iter().position(|a| a == "--smoke") {
        let seconds = args
            .get(index + 1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(5)
            .clamp(1, 1800);
        let result = (|| -> Result<(), String> {
            let devices = audio::list_devices()?;
            let input = devices
                .iter()
                .find(|d| d.input && d.is_default && !d.name.contains("CABLE"))
                .or_else(|| {
                    devices
                        .iter()
                        .find(|d| d.input && !d.name.contains("CABLE"))
                })
                .ok_or("没有真实麦克风")?;
            let output = devices
                .iter()
                .find(|d| !d.input && d.name.contains("CABLE Input"))
                .ok_or("未安装 VB-CABLE，无法执行虚拟麦克风硬件检查")?;
            let settings = Session {
                input_id: input.id.clone(),
                output_id: output.id.clone(),
                ..Session::default()
            };
            let mut controller = Controller::default();
            controller.start(&settings)?;
            for i in 0..seconds * 5 {
                if i == 5 {
                    let filters:Vec<FilterConfig>=serde_json::from_str(r#"[{"id":"smoke-frequency","type":"frequency","enabled":true,"params":{"shift_hz":100}},{"id":"smoke-gain","type":"gain","enabled":true,"params":{"gain_db":-6}}]"#).map_err(|e|e.to_string())?;
                    controller.set_chain(&filters)?;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                let status = controller.status();
                if !status.running {
                    return Err(status.error.unwrap_or("音频停止".into()));
                }
                if i == 4 || i % 50 == 49 {
                    println!("{}", serde_json::to_string(&status).unwrap());
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&controller.status()).unwrap()
            );
            controller.stop();
            Ok(())
        })();
        return Some(match result {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                2
            }
        });
    }
    None
}
fn main() {
    if let Some(code) = diagnostic() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .setup(|app| {
            let store = Store::new(directory(app)?);
            app.manage(NativeState {
                audio: Mutex::new(Controller::default()),
                store: Mutex::new(store),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            load_session,
            save_session,
            set_chain,
            start_audio,
            stop_audio,
            set_monitoring,
            get_status,
            install_cable,
            open_cable_site
        ])
        .run(tauri::generate_context!())
        .expect("desktop application failed");
}
