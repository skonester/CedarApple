use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub engine: String,
    pub entry: String,
    pub view: String,
    pub source: String,
}

#[derive(Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub entry: PathBuf,
}

pub fn read(path: &Path) -> Result<Installed, String> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid Slice manifest: {e}"))?;
    if manifest.schema != 1 || manifest.engine != "mpv-lua" || manifest.view != "search-grid" {
        return Err("This Slice requires an unsupported schema, engine or view".into());
    }
    if manifest.id.is_empty()
        || manifest.id.len() > 64
        || !manifest
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Slice IDs must contain lowercase letters, numbers and hyphens".into());
    }
    let relative = Path::new(&manifest.entry);
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        || relative.extension().and_then(|s| s.to_str()) != Some("lua")
    {
        return Err("The Slice entry must be a relative Lua file inside its folder".into());
    }
    let folder = path
        .parent()
        .ok_or("Missing Slice folder")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let entry = folder
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("Missing Slice script: {e}"))?;
    if !entry.starts_with(&folder) || !entry.is_file() {
        return Err("The Slice entry must stay inside its folder".into());
    }
    Ok(Installed { manifest, entry })
}

pub fn user_dir() -> Option<PathBuf> {
    Some(
        directories::ProjectDirs::from("", "", "CedarApple")?
            .config_dir()
            .join("slices"),
    )
}

pub fn discover() -> (Vec<Installed>, Vec<String>) {
    let executable = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let bundled = executable
        .as_ref()
        .map(|p| p.join("mpv/slices"))
        .filter(|p| p.is_dir())
        .or_else(|| {
            executable
                .as_ref()
                .map(|p| p.join("../share/cedarapple/mpv/slices"))
                .filter(|p| p.is_dir())
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/mpv/slices"));
    let mut folders = vec![bundled];
    folders.extend(user_dir());
    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for root in folders {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path().join("slice.json"))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            match read(&path) {
                Ok(slice) if ids.insert(slice.manifest.id.clone()) => installed.push(slice),
                Ok(slice) => errors.push(format!("Duplicate Slice ID: {}", slice.manifest.id)),
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
    installed.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    (installed, errors)
}

/// Lua byte escapes avoid treating paths, quotes or Unicode as source code.
pub fn lua_string(value: &str) -> String {
    let mut out = String::from("\"");
    for byte in value.as_bytes() {
        out.push_str(&format!("\\{byte:03}"));
    }
    out.push('"');
    out
}

pub fn loader(slice: &Installed, session: &str) -> String {
    let config = serde_json::json!({"id": slice.manifest.id, "session": session, "entry": slice.entry.to_string_lossy()});
    format!(
        "CEDAR_SLICE_CONFIG = {}\n{}",
        lua_string(&config.to_string()),
        include_str!("../../assets/mpv/slices/host.lua")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_bundled_slice_and_keeps_upstream_attribution() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/mpv/slices/youtube-search/slice.json");
        let slice = read(&path).unwrap();
        assert_eq!(
            slice.manifest.source,
            "https://github.com/Lunedor/mpv-youtube-search"
        );
        assert!(slice.entry.is_file());
    }
    #[test]
    fn quotes_lua_values_as_bytes() {
        assert_eq!(lua_string("a\"\\\n"), "\"\\097\\034\\092\\010\"");
        assert_eq!(lua_string("é"), "\"\\195\\169\"");
    }
    #[test]
    fn rejects_entries_outside_the_slice_folder() {
        let dir = std::env::temp_dir().join(format!("cedar-slice-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../assets/mpv/slices/youtube-search/slice.json"
        ))
        .unwrap();
        value["entry"] = "../outside.lua".into();
        let path = dir.join("slice.json");
        std::fs::write(&path, value.to_string()).unwrap();
        assert!(read(&path).is_err());
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }
}
