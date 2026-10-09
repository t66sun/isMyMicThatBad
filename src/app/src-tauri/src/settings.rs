use crate::dsp::{self, FilterConfig};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub chain: Vec<FilterConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub schema_version: u32,
    pub input_id: String,
    pub output_id: String,
    pub monitor_id: String,
    pub monitoring: bool,
    pub buffer_ms: u32,
    pub chain: Vec<FilterConfig>,
    pub presets: Vec<Preset>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            schema_version: 1,
            input_id: String::new(),
            output_id: String::new(),
            monitor_id: String::new(),
            monitoring: false,
            buffer_ms: 5,
            chain: Vec::new(),
            presets: Vec::new(),
        }
    }
}
impl Session {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !(5..=30).contains(&self.buffer_ms)
            || self.presets.len() > 100
            || [&self.input_id, &self.output_id, &self.monitor_id]
                .iter()
                .any(|s| s.len() > 1024)
        {
            return Err("配置格式或缓冲范围无效".into());
        }
        dsp::validate(&self.chain)?;
        let mut ids = std::collections::HashSet::new();
        for preset in &self.presets {
            if preset.id.is_empty()
                || !ids.insert(&preset.id)
                || preset.name.trim().is_empty()
                || preset.name.chars().count() > 80
            {
                return Err("预设格式无效".into());
            }
            dsp::validate(&preset.chain)?;
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Loaded {
    pub session: Session,
    pub warning: Option<String>,
}
pub struct Store {
    directory: PathBuf,
}
impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn load(&self) -> Result<Loaded, String> {
        let path = self.directory.join("settings.json");
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    session: Session::default(),
                    warning: None,
                })
            }
            Err(error) => return Err(format!("读取配置失败：{error}")),
        };
        match serde_json::from_str::<Session>(&content).and_then(|session| {
            session.validate().map_err(serde::de::Error::custom)?;
            Ok(session)
        }) {
            Ok(session) => Ok(Loaded {
                session,
                warning: None,
            }),
            Err(_) => {
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos();
                let backup = self
                    .directory
                    .join(format!("settings.corrupt-{nonce}.json"));
                fs::rename(&path, &backup)
                    .map_err(|e| format!("无效配置无法备份，已停止自动保存以保留原文件：{e}"))?;
                Ok(Loaded {
                    session: Session::default(),
                    warning: Some(format!(
                        "配置无效，已保留备份 {} 并恢复默认。",
                        backup.display()
                    )),
                })
            }
        }
    }

    pub fn save(&self, session: &Session) -> Result<(), String> {
        session.validate()?;
        fs::create_dir_all(&self.directory).map_err(|e| format!("无法创建配置目录：{e}"))?;
        let bytes = serde_json::to_vec_pretty(session).map_err(|e| e.to_string())?;
        let temporary = self.directory.join("settings.json.tmp");
        {
            use std::io::Write;
            let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(temporary, self.directory.join("settings.json"))
            .map_err(|e| format!("保存配置失败，原配置保持不变：{e}"))?;
        Ok(())
    }
}
