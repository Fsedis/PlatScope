//! Общий справочник имён для карты и билдов. Читает только локальную БД,
//! не обращается к памяти игры и не влияет на обновление координат.
use crate::AppState;
use platscope_domain::{GameMetadataSnapshot, ItemCatalog};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct GameNames(HashMap<String, (String, String, String)>);

impl GameNames {
    fn merge(&mut self, path: &str, ru: Option<&str>, en: &str, kind: &str) {
        let entry = self.0.entry(normalize_path(path)).or_default();
        if let Some(ru) = ru.map(str::trim).filter(|s| !s.is_empty()) {
            entry.0 = ru.into();
        }
        if !en.trim().is_empty() {
            entry.1 = en.trim().into();
        }
        if kind != "unknown" || entry.2.is_empty() {
            entry.2 = kind.into();
        }
    }

    pub(crate) fn new(
        catalog: Option<&ItemCatalog>,
        metadata: Option<&GameMetadataSnapshot>,
    ) -> Self {
        let mut names = Self::default();
        if let Some(metadata) = metadata {
            for relic in &metadata.relics {
                names.merge(&relic.relic_game_ref, None, &relic.display_name_en, "relic");
            }
            for item in &metadata.mastery_items {
                names.merge(
                    &item.game_ref,
                    item.display_name_ru.as_deref(),
                    &item.display_name_en,
                    "equipment",
                );
            }
            for item in &metadata.syndicate_offers {
                names.merge(
                    &item.game_ref,
                    item.display_name_ru.as_deref(),
                    &item.display_name_en,
                    "unknown",
                );
            }
        }
        if let Some(catalog) = catalog {
            for item in &catalog.items {
                if let Some(path) = &item.game_ref {
                    let kind = if item.tags.iter().any(|t| t == "arcane_enhancement") {
                        "arcane"
                    } else if item.tags.iter().any(|t| t == "mod") {
                        "mod"
                    } else {
                        "unknown"
                    };
                    names.merge(
                        path,
                        item.display_name_ru.as_deref(),
                        &item.display_name_en,
                        kind,
                    );
                }
            }
        }
        // Перевод игры имеет приоритет; английское имя и классификация сохраняются.
        if let Some(metadata) = metadata {
            for item in &metadata.item_localizations {
                names.merge(&item.game_ref, Some(&item.display_name_ru), "", "unknown");
            }
        }
        names
    }

    pub(crate) fn lookup(&self, path: &str) -> Option<&(String, String, String)> {
        self.0.get(&normalize_path(path))
    }
}

pub(crate) fn normalize_path(path: &str) -> String {
    path.strip_prefix("/Lotus/StoreItems/")
        .map_or_else(|| path.to_owned(), |s| format!("/Lotus/{s}"))
}

#[derive(Default)]
pub(crate) struct Cache {
    names: Arc<GameNames>,
    checked_at: Option<Instant>,
}

pub(crate) fn get(state: &AppState) -> Arc<GameNames> {
    let Ok(mut cache) = state.game_names.lock() else {
        return Arc::default();
    };
    if cache
        .checked_at
        .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
    {
        // Даже при ошибке БД не повторяем тяжёлую загрузку на каждом кадре карты.
        cache.checked_at = Some(Instant::now());
        if let Ok(db) = state.inventory_database.lock()
            && let (Ok(catalog), Ok(metadata)) =
                (db.load_current_catalog(), db.load_current_game_metadata())
        {
            cache.names = Arc::new(GameNames::new(catalog.as_ref(), metadata.as_ref()));
        }
    }
    cache.names.clone()
}

#[cfg(test)]
pub(crate) fn test_names() -> GameNames {
    let metadata: GameMetadataSnapshot = serde_json::from_value(serde_json::json!({
        "metadata":{"source":"wfcd_warframe_items","fetchedAt":"2026-09-12T00:00:00Z","schemaVersion":1,"setCount":0,"relicCount":0,"primePartCount":0,"checksumSha256":"test"},
        "primeSets":[],"relics":[],"primeParts":[],
        "masteryItems":[{"gameRef":"/Lotus/Test","displayNameEn":"Test","displayNameRu":null,"category":"Warframes","imageUrl":null,"maxRank":30}],
        "itemLocalizations":[{"gameRef":"/Lotus/Test","displayNameRu":"Чертёж"}]
    })).unwrap();
    GameNames::new(None, Some(&metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translations_keep_english_names_and_types_across_sources() {
        let mut names = GameNames::default();
        names.merge("/Lotus/StoreItems/Test", Some("Старое имя"), "Test", "mod");
        names.merge("/Lotus/Test", None, "Test", "unknown");
        names.merge("/Lotus/Test", Some("Игровой перевод"), "", "unknown");
        assert_eq!(
            names.lookup("/Lotus/StoreItems/Test"),
            Some(&("Игровой перевод".into(), "Test".into(), "mod".into()))
        );
        assert!(names.lookup("/Lotus/Missing").is_none());
    }
}
