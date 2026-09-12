use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
pub(crate) struct Field {
    types: BTreeSet<&'static str>,
    array_lengths: BTreeSet<usize>,
    examples: BTreeSet<String>,
}

fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key.as_bytes()[0].is_ascii_alphabetic()
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && ![
            "account",
            "nonce",
            "token",
            "password",
            "email",
            "address",
            "displayname",
            "playername",
            "kubrowname",
            "guild",
            "clan",
            "session",
            "challenge",
        ]
        .iter()
        .any(|s| key.to_ascii_lowercase().contains(s))
}

pub(crate) fn walk(path: &str, value: &Value, fields: &mut BTreeMap<String, Field>, depth: usize) {
    if depth > 12 || fields.len() > 3000 {
        return;
    }
    let field = fields.entry(path.into()).or_default();
    field.types.insert(match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    });
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if safe_key(key) {
                    walk(&format!("{path}.{key}"), value, fields, depth + 1);
                }
            }
        }
        Value::Array(array) => {
            field.array_lengths.insert(array.len());
            for value in array.iter().take(256) {
                walk(&format!("{path}[]"), value, fields, depth + 1);
            }
        }
        Value::String(text) => {
            if path.ends_with(".UpgradeFingerprint") && text.starts_with('{') && text.len() < 8192 {
                if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                    walk(&format!("{path}.decoded"), &parsed, fields, depth + 1);
                }
                return;
            }
            let key = path.rsplit('.').next().unwrap_or("");
            let enum_value = ["missionType", "Color", "Tag"].contains(&key)
                && text.len() <= 64
                && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
            let color_value = path.contains("Colors.")
                && text.len() == 8
                && text.bytes().all(|b| b.is_ascii_hexdigit());
            if field.examples.len() < 12 && (enum_value || color_value) {
                field.examples.insert(text.clone());
            }
            if field.examples.len() < 12
                && text.starts_with("/Lotus/")
                && text.len() < 512
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
            {
                field.examples.insert(text.clone());
            }
        }
        Value::Bool(_) | Value::Number(_) => {
            // Значения только известных игровых параметров, остальные — лишь тип.
            let key = path.rsplit('.').next().unwrap_or("").trim_end_matches("[]");
            if [
                "Index",
                "Progress",
                "Level",
                "Polarized",
                "PlayerLevel",
                "Count",
                "Amount",
                "Rank",
                "PostNewWar",
                "PostOldPeace",
                "Slot",
                "Health",
                "Energy",
                "Strength",
                "Duration",
                "Range",
                "Efficiency",
                "Assigned",
                "AssignedRole",
                "SecondInCommand",
                "lvl",
                "charges",
                "Value",
                "r",
                "g",
                "b",
                "a",
                "t0",
                "t1",
                "t2",
                "t3",
                "t4",
                "m0",
                "m1",
            ]
            .contains(&key)
                && field.examples.len() < 12
            {
                field.examples.insert(value.to_string());
            }
        }
        Value::Null => {}
    }
}

// Сохраняем связь предмета, конфигурации и игровых параметров без идентификаторов.
#[allow(dead_code)]
pub(crate) fn suit_details(item: &Value) -> Value {
    let mut fields = BTreeMap::new();
    for (index, shard) in item["ArchonCrystalUpgrades"]
        .as_array()
        .into_iter()
        .flatten()
        .take(5)
        .enumerate()
    {
        walk(
            &format!("item.ArchonCrystalUpgrades[{index}]"),
            shard,
            &mut fields,
            0,
        );
    }
    for key in ["ItemType", "ArchonCrystalUpgrades"] {
        if let Some(value) = item.get(key) {
            walk(&format!("item.{key}"), value, &mut fields, 0);
        }
    }
    for (index, config) in item
        .get("Configs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(12)
        .enumerate()
    {
        if let Some(value) = config.get("AbilityOverride") {
            walk(
                &format!("item.Configs[{index}].AbilityOverride"),
                value,
                &mut fields,
                0,
            );
        }
    }
    serde_json::to_value(fields).expect("Схема состоит из сериализуемых полей")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_omits_personal_values_and_dynamic_identifiers() {
        let value = serde_json::json!({"PlayerName":"private-name", "AccountId":"secret", "Misc":"also-secret", "ItemType":"/Lotus/Test", "24abcdef0123456789abcdef":{"Value":"hidden"}, "NORMAL":[{"Level":30}]});
        let mut fields = BTreeMap::new();
        walk("loadout", &value, &mut fields, 0);
        let text = serde_json::to_string(&fields).unwrap();
        for forbidden in [
            "secret",
            "private-name",
            "24abcdef",
            "AccountId",
            "PlayerName",
        ] {
            assert!(!text.contains(forbidden));
        }
        assert!(text.contains("/Lotus/Test"));
        assert!(text.contains("30"));
    }

    #[test]
    fn suit_details_preserve_configuration_without_identifiers() {
        let item = serde_json::json!({"ItemType":"/Lotus/TestSuit", "ItemId":"private-id", "Configs":[{"Name":"private-title", "AbilityOverride":{"Ability":"/Lotus/TestAbility", "Index":2}}], "ArchonCrystalUpgrades":[{"Color":"ACC_RED", "UpgradeType":"/Lotus/TestUpgrade"}]});
        let details = suit_details(&item);
        assert_eq!(
            details["item.Configs[0].AbilityOverride.Index"]["examples"][0],
            "2"
        );
        let text = details.to_string();
        assert!(text.contains("/Lotus/TestSuit"));
        assert!(text.contains("ACC_RED"));
        assert!(!text.contains("private"));
    }
}
