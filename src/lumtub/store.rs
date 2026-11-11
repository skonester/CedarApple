//! Local watch later, history and followed channels. No Google account needed.
use super::catalog::Item;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub saved: Vec<Item>,
    pub history: Vec<Item>,
    pub channels: Vec<Item>,
}

impl Store {
    fn path() -> Option<std::path::PathBuf> {
        Some(
            directories::ProjectDirs::from("", "", "CedarApple")?
                .data_dir()
                .join("lumtub.json"),
        )
    }
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Could not find CedarApple's data folder")?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
    pub fn toggle(list: &mut Vec<Item>, item: Item) {
        if list.iter().any(|i| i.id == item.id) {
            list.retain(|i| i.id != item.id);
        } else {
            list.insert(0, item);
        }
    }
    pub fn watched(&mut self, item: Item) {
        self.history.retain(|i| i.id != item.id);
        self.history.insert(0, item);
        self.history.truncate(200);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_order_and_deduplicates_history() {
        let mut store = Store::default();
        let a = Item {
            id: "a".into(),
            ..Default::default()
        };
        let b = Item {
            id: "b".into(),
            ..Default::default()
        };
        store.watched(a.clone());
        store.watched(b);
        store.watched(a.clone());
        assert_eq!(store.history.len(), 2);
        assert_eq!(store.history[0].id, "a");
        Store::toggle(&mut store.saved, a.clone());
        Store::toggle(&mut store.saved, a);
        assert!(store.saved.is_empty());
        assert!(
            serde_json::from_str::<Store>("{}")
                .unwrap()
                .channels
                .is_empty()
        );
    }
}
