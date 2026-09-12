//! Отряд, снимки экипировки и личная коллекция билдов. Сырой JSON процесса
//! не сохраняется: только перечисленные ниже поля, без адресов и идентификаторов.
use crate::AppState;
use crate::game_names::{self, GameNames};
use chrono::Utc;
use platscope_readonly_scan::{
    scan::find_wf_pid,
    squad::{Candidate, capture, capture_suits},
    squad_log::LogParser,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, State};

const SAVED_KEY: &str = "squad.saved_builds.v1";
const ENABLED_KEY: &str = "squad.enabled.v1";
const MAX_SAVED: usize = 200;

fn export_text(directory: &std::path::Path, text: &str) -> Result<String, String> {
    use std::io::Write;
    if text.is_empty() || text.len() > 256 * 1024 {
        return Err("Список пуст или превышает допустимый размер.".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join(format!(
        "build-{}.txt",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_export_text(
    text: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    export_text(&state.data_directory.join("build-exports"), &text)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Part {
    path: String,
    name: String,
    name_en: String,
    kind: String,
    // Только явно переданный ранг. Номер элемента WeaponUpgrades не является слотом мода.
    rank: Option<u64>,
    #[serde(default)]
    slot_index: Option<usize>,
    #[serde(default)]
    fingerprint: Option<RivenFingerprint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RivenFingerprint {
    weapon_path: Option<String>,
    weapon_name: Option<String>,
    weapon_name_en: Option<String>,
    mastery_rank: Option<u64>,
    rerolls: Option<u64>,
    polarity: Option<String>,
    buffs: Vec<RivenStat>,
    curses: Vec<RivenStat>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RivenStat {
    tag: String,
    value: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpgradeSlot {
    index: usize,
    status: String,
    part: Option<Part>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Equipment {
    key: String,
    category: String,
    item: Part,
    level: Option<u64>,
    forma: Option<u64>,
    upgrades: Vec<Part>,
    modular_parts: Vec<Part>,
    unreadable_upgrades: usize,
    #[serde(default)]
    shards: Option<Vec<Shard>>,
    #[serde(default)]
    ability_override: Option<AbilityOverride>,
    #[serde(default)]
    configuration: Option<usize>,
    #[serde(default)]
    source: String,
    #[serde(default)]
    context: LoadoutContext,
    #[serde(default)]
    upgrade_slots: Option<Vec<UpgradeSlot>>,
    #[serde(default)]
    inventory_resolved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Shard {
    color: String,
    effect: Part,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityOverride {
    ability: Part,
    slot: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoadoutContext {
    focus: Option<Part>,
    relic: Option<Part>,
    refinement: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Member {
    name: String,
    platform: String,
    is_host: bool,
    status: String,
    captured_at: Option<String>,
    equipment: Vec<Equipment>,
    mastery_rank: Option<u64>,
    #[serde(skip)]
    expected_bytes: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedBuild {
    id: String,
    title: String,
    player: String,
    platform: String,
    captured_at: String,
    saved_at: String,
    note: String,
    equipment: Equipment,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // Независимые признаки настройки, игры и двух видов чтения.
pub(crate) struct View {
    enabled: bool,
    running: bool,
    scanning: bool,
    error: Option<String>,
    members: Vec<Member>,
    saved: Vec<SavedBuild>,
    configurations: Vec<Equipment>,
    configurations_at: Option<String>,
    reading_configurations: bool,
}

// Независимые признаки: настройка, очередь чтения, выполняющееся чтение и конец журнала.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Service {
    parser: LogParser,
    tail: String,
    enabled: bool,
    pid: Option<u32>,
    revision: u64,
    pending: bool,
    attempts: u8,
    scanning: bool,
    error: Option<String>,
    members: Vec<Member>,
    log_ready: bool,
    configurations: Vec<Equipment>,
    configurations_at: Option<String>,
    reading_configurations: bool,
    configurations_revision: u64,
}

impl Default for Service {
    fn default() -> Self {
        Self {
            parser: LogParser::default(),
            tail: String::new(),
            enabled: true,
            pid: None,
            revision: 0,
            pending: false,
            attempts: 0,
            scanning: false,
            error: None,
            members: Vec::new(),
            log_ready: false,
            configurations: Vec::new(),
            configurations_at: None,
            reading_configurations: false,
            configurations_revision: 0,
        }
    }
}

impl Service {
    pub(crate) fn reset(&mut self) {
        self.parser = LogParser::default();
        self.tail.clear();
        self.members.clear();
        self.pending = false;
        self.revision += 1;
        self.error = None;
        self.log_ready = false;
        self.configurations.clear();
        self.configurations_at = None;
        self.configurations_revision += 1;
    }

    pub(crate) fn feed(&mut self, chunk: &str) {
        self.tail.push_str(chunk);
        while let Some(end) = self.tail.find('\n') {
            let line = self.tail[..end].to_owned();
            self.tail.drain(..=end);
            if self.parser.process_line(&line) {
                self.reconcile();
            }
        }
        if self.tail.len() > 64 * 1024 {
            self.tail.clear();
        }
    }

    pub(crate) fn set_log_ready(&mut self, ready: bool) {
        self.log_ready = ready;
    }

    fn reconcile(&mut self) {
        self.revision += 1;
        self.pending = self.enabled;
        self.attempts = 0;
        let previous = std::mem::take(&mut self.members);
        self.members = self
            .parser
            .peers()
            .iter()
            .filter(|p| Some(p.name.as_str()) != self.parser.local_user())
            .take(8)
            .map(|p| {
                let mut member = previous
                    .iter()
                    .find(|m| m.name == p.name && m.expected_bytes == p.loadout_bytes)
                    .cloned()
                    .unwrap_or(Member {
                        name: p.name.clone(),
                        platform: p.platform.label().into(),
                        is_host: p.is_host,
                        status: "waiting".into(),
                        captured_at: None,
                        equipment: Vec::new(),
                        mastery_rank: None,
                        expected_bytes: p.loadout_bytes,
                    });
                member.is_host = p.is_host;
                member
            })
            .collect();
    }
}

#[derive(Default)]
struct Names(Arc<GameNames>);
impl Names {
    fn localize_part(&self, part: &mut Part) {
        let translated = self.part(&part.path);
        if !translated.name.is_empty()
            && (translated.name != translated.name_en
                || part.name.is_empty()
                || part.name == part.name_en)
        {
            part.name = translated.name;
        }
        if !translated.name_en.is_empty() {
            part.name_en = translated.name_en;
        }
        if let Some(fingerprint) = &mut part.fingerprint
            && let Some(path) = &fingerprint.weapon_path
        {
            let weapon = self.part(path);
            if !weapon.name.is_empty() {
                fingerprint.weapon_name = Some(weapon.name);
            }
            if !weapon.name_en.is_empty() {
                fingerprint.weapon_name_en = Some(weapon.name_en);
            }
        }
    }

    fn localize_equipment(&self, equipment: &mut Equipment) {
        self.localize_part(&mut equipment.item);
        for part in equipment
            .upgrades
            .iter_mut()
            .chain(&mut equipment.modular_parts)
        {
            self.localize_part(part);
        }
        for slot in equipment.upgrade_slots.iter_mut().flatten() {
            if let Some(part) = &mut slot.part {
                self.localize_part(part);
            }
        }
        for shard in equipment.shards.iter_mut().flatten() {
            self.localize_part(&mut shard.effect);
        }
        if let Some(ability) = &mut equipment.ability_override {
            self.localize_part(&mut ability.ability);
        }
        for part in [&mut equipment.context.focus, &mut equipment.context.relic]
            .into_iter()
            .flatten()
        {
            self.localize_part(part);
        }
    }

    fn part(&self, path: &str) -> Part {
        let normalized = game_names::normalize_path(path);
        let (mut name, mut en, mut kind) = self
            .0
            .lookup(&normalized)
            .cloned()
            .unwrap_or_else(|| (String::new(), String::new(), "unknown".into()));
        if name.is_empty()
            && let Some((ru, english)) = known_detail(&normalized)
        {
            name = ru.into();
            en = english.into();
        }
        if name.is_empty() {
            name.clone_from(&en);
        }
        if kind == "unknown" {
            kind = if normalized.contains("/Upgrades/Mods/") {
                "mod"
            } else if normalized.contains("/Upgrades/CosmeticEnhancers/") {
                "arcane"
            } else if normalized.contains("/Skins/") || normalized.contains("/Customization/") {
                "cosmetic"
            } else {
                "unknown"
            }
            .into();
        }
        Part {
            path: normalized,
            name,
            name_en: en,
            kind,
            rank: None,
            slot_index: None,
            fingerprint: None,
        }
    }
}

fn known_detail(path: &str) -> Option<(&'static str, &'static str)> {
    let key = path.rsplit('/').next()?;
    Some(match key {
        "RhinoRoarAbility" => ("Рёв", "Roar"),
        "HopliteBashAbility" => ("Удар Тарроса", "Tharros Strike"),
        "DevourerConsumeAbility" => ("Подпитка", "Nourish"),
        "RunnerTransferAbility" => ("Термальный раскол", "Thermal Sunder"),
        "OdaliskDispensaryAbility" => ("Диспенсарий", "Dispensary"),
        "RevenantAfflictionAbility" => ("Опустошение", "Reave"),
        "BardCharmAbility" => ("Резонатор", "Resonator"),
        "HelminthSpeedAbility" => ("Ускорение Гельминта", "Infested Mobility"),
        "AttackFocusAbility" => ("Мадурай", "Madurai"),
        "PowerFocusAbility" => ("Зенурик", "Zenurik"),
        "ArchonCrystalUpgradeWarframeAbilityDuration" => {
            ("Длительность способностей", "Ability Duration")
        }
        "ArchonCrystalUpgradeWarframeAbilityStrength" => ("Сила способностей", "Ability Strength"),
        "ArchonCrystalUpgradeWarframeCastingSpeed" => {
            ("Скорость применения способностей", "Casting Speed")
        }
        "ArchonCrystalUpgradeWarframeEnergyMax" | "ArchonCrystalUpgradeWarframeEnergyMaxMythic" => {
            ("Максимальная энергия", "Energy Max")
        }
        "ArchonCrystalUpgradeWarframeGlobeEffectEnergy" => {
            ("Эффективность сфер энергии", "Energy Orb Effectiveness")
        }
        "ArchonCrystalUpgradeWarframeParkourVelocity"
        | "ArchonCrystalUpgradeWarframeParkourVelocityMythic" => {
            ("Скорость паркура", "Parkour Velocity")
        }
        "ArchonCrystalUpgradeWarframeStartingEnergy" => {
            ("Энергия в начале миссии", "Starting Energy")
        }
        _ => return None,
    })
}

fn game_path(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| {
        s.starts_with("/Lotus/")
            && s.len() <= 1024
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
    })
}

fn shards(value: &Value, names: &Names) -> Option<Vec<Shard>> {
    let values = value.as_array()?;
    if values.len() > 5 {
        return None;
    }
    values
        .iter()
        .map(|v| {
            Some(Shard {
                color: v["Color"]
                    .as_str()
                    .filter(|s| {
                        s.starts_with("ACC_")
                            && s.len() < 40
                            && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                    })?
                    .into(),
                effect: names.part(game_path(&v["UpgradeType"])?),
            })
        })
        .collect()
}

fn ability_override(value: &Value, names: &Names) -> Option<AbilityOverride> {
    Some(AbilityOverride {
        ability: names.part(game_path(&value["Ability"])?),
        slot: value["Index"].as_u64().filter(|n| *n < 4).map(|n| n + 1),
    })
}

fn loadout_context(value: &Value, names: &Names) -> LoadoutContext {
    LoadoutContext {
        focus: game_path(&value["FocusAbility"]).map(|p| names.part(p)),
        relic: game_path(&value["VoidProjection"]["ItemType"]).map(|p| names.part(p)),
        refinement: value["VoidProjection"]["Level"].as_u64().filter(|n| *n < 4),
    }
}

fn equipment(value: &Value, names: &Names) -> Vec<Equipment> {
    let mut items = Vec::new();
    for (group, label) in [
        ("NORMAL", "Снаряжение"),
        ("SENTINEL", "Компаньон"),
        ("ARCHWING", "Арчвинг"),
        ("MECH", "Некрамех"),
        ("OPERATOR", "Оператор"),
        ("OPERATOR_ADULT", "Скиталец"),
        ("DATAKNIFE", "Прочее снаряжение"),
    ] {
        for (index, item) in value[group]
            .as_array()
            .into_iter()
            .flatten()
            .take(16)
            .enumerate()
        {
            let Some(path) = item["ItemType"]
                .as_str()
                .filter(|s| s.starts_with("/Lotus/") && s.len() <= 1024)
            else {
                continue;
            };
            let mut upgrades = Vec::new();
            let mut unreadable = 0;
            for upgrade in item["WeaponUpgrades"]
                .as_array()
                .into_iter()
                .flatten()
                .take(256)
            {
                if let Some(path) = upgrade.as_str() {
                    if path.is_empty() {
                        continue;
                    }
                    if path.starts_with("/Lotus/") && path.len() <= 1024 {
                        upgrades.push(names.part(path));
                    } else {
                        unreadable += 1;
                    }
                } else {
                    unreadable += 1;
                }
            }
            if !item["WeaponUpgrades"].is_array() {
                unreadable += 1;
            }
            let category = if group == "NORMAL" {
                match index {
                    0 => "Варфрейм",
                    1 => "Дополнительное оружие",
                    2 => "Основное оружие",
                    3 => "Ближний бой",
                    4 => "Арчган",
                    _ => label,
                }
            } else {
                label
            };
            items.push(Equipment {
                key: format!("{group}:{index}"),
                category: category.into(),
                item: names.part(path),
                level: item["Level"].as_u64(),
                forma: item["Polarized"].as_u64(),
                upgrades,
                modular_parts: item["ModularPartTypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|p| p.starts_with("/Lotus/") && p.len() <= 1024)
                    .take(16)
                    .map(|p| names.part(p))
                    .collect(),
                unreadable_upgrades: unreadable,
                shards: shards(&item["ArchonCrystalUpgrades"], names),
                ability_override: ability_override(&item["AbilityOverride"], names),
                configuration: None,
                source: "squad".into(),
                context: loadout_context(value, names),
                upgrade_slots: None,
                inventory_resolved: false,
            });
        }
    }
    items
}

fn fingerprint_value(item: &Value) -> Option<Value> {
    match &item["UpgradeFingerprint"] {
        Value::String(s) if s.len() <= 16 * 1024 => serde_json::from_str(s).ok(),
        Value::Object(_) => Some(item["UpgradeFingerprint"].clone()),
        _ => None,
    }
    .filter(Value::is_object)
}

fn upgrade_part(item: &Value, names: &Names) -> Option<Part> {
    let mut part = names.part(game_path(&item["ItemType"])?);
    if let Some(value) = fingerprint_value(item) {
        part.rank = value["lvl"].as_u64().filter(|n| *n <= 100);
        if part.path.contains("/Randomized/") || value["compat"].is_string() {
            let weapon = game_path(&value["compat"]).map(|p| names.part(p));
            let stats = |key: &str| {
                value[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(16)
                    .filter_map(|s| {
                        let tag = s["Tag"].as_str()?.to_owned();
                        if tag.len() > 128
                            || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        {
                            return None;
                        }
                        Some(RivenStat {
                            tag,
                            value: s["Value"].as_f64().filter(|n| n.is_finite()),
                        })
                    })
                    .collect()
            };
            part.fingerprint = Some(RivenFingerprint {
                weapon_path: weapon.as_ref().map(|p| p.path.clone()),
                weapon_name: weapon.as_ref().map(|p| p.name.clone()),
                weapon_name_en: weapon.as_ref().map(|p| p.name_en.clone()),
                mastery_rank: value["lvlReq"].as_u64().filter(|n| *n <= 100),
                rerolls: value["rerolls"].as_u64(),
                polarity: value["pol"]
                    .as_str()
                    .filter(|s| {
                        s.len() <= 40 && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                    })
                    .map(str::to_owned),
                buffs: stats("buffs"),
                curses: stats("curses"),
            });
        }
    }
    Some(part)
}

fn gear_category(group: &str) -> &str {
    match group {
        "Suits" => "Варфрейм",
        "LongGuns" => "Основное оружие",
        "Pistols" => "Дополнительное оружие",
        "Melee" => "Ближний бой",
        "Sentinels" => "Страж",
        "SentinelWeapons" => "Оружие стража",
        "KubrowPets" | "KavatPets" => "Питомец",
        "MoaPets" => "Робот-компаньон",
        "KubrowPetWeapons" | "KavatPetWeapons" => "Оружие питомца",
        "SpaceSuits" => "Арчвинг",
        "SpaceGuns" => "Арчган",
        "SpaceMelee" => "Арчмили",
        "MechSuits" => "Некрамех",
        "OperatorAmps" => "Усилитель",
        "OperatorSuits" => "Оператор и Скиталец",
        "DrifterMelee" => "Оружие Скитальца",
        "DataKnives" => "Паразон",
        "CrewShipHarnesses" => "Плексус",
        "CrewShips" => "Рейлджек",
        "SpecialItems" => "Особое снаряжение",
        "Scoops" => "Снаряжение Лунаро",
        "Horses" => "Кэйт",
        "Hoverboards" => "К-Драйв",
        "Motorcycles" => "Атомицикл",
        _ => "Другое снаряжение",
    }
}

fn configuration_item(
    gear: &Value,
    config: &Value,
    index: usize,
    group: &str,
    mods: &HashMap<&str, Option<&Value>>,
    names: &Names,
) -> Option<Equipment> {
    let value = serde_json::json!({"NORMAL":[{
        "ItemType":gear["ItemType"], "Level":gear["Level"], "Polarized":gear["Polarized"],
        "WeaponUpgrades":[], "ArchonCrystalUpgrades":gear["ArchonCrystalUpgrades"],
        "AbilityOverride":config["AbilityOverride"], "ModularPartTypes":gear["ModularPartTypes"]
    }]});
    let mut item = equipment(&value, names).into_iter().next()?;
    item.category = gear_category(group).into();
    item.configuration = Some(index + 1);
    item.source = "memoryConfiguration".into();
    item.upgrade_slots = config["Upgrades"].as_array().map(|refs| {
        refs.iter()
            .take(256)
            .enumerate()
            .map(|(slot, reference)| {
                let text = reference.as_str();
                let empty = text == Some("");
                let part = text.and_then(|s| {
                    if let Some(path) = game_path(reference) {
                        Some(names.part(path))
                    } else {
                        mods.get(s)
                            .and_then(|v| *v)
                            .and_then(|v| upgrade_part(v, names))
                    }
                });
                let part = part.map(|mut p| {
                    p.slot_index = Some(slot);
                    p
                });
                if let Some(p) = &part {
                    item.upgrades.push(p.clone());
                }
                if !empty && part.is_none() {
                    item.unreadable_upgrades += 1;
                }
                UpgradeSlot {
                    index: slot,
                    status: if empty {
                        "empty"
                    } else if part.is_some() {
                        "resolved"
                    } else {
                        "unresolved"
                    }
                    .into(),
                    part,
                }
            })
            .collect()
    });
    if item.upgrade_slots.is_none() {
        item.unreadable_upgrades += 1;
    }
    Some(item)
}

fn configurations(candidates: &[Candidate], names: &Names) -> Vec<Equipment> {
    let mut unique = std::collections::BTreeMap::new();
    for candidate in candidates {
        // Индекс ограничен одним цельным инвентарём: старые копии не смешиваются.
        let mut mods: HashMap<&str, Option<&Value>> = HashMap::new();
        let full = candidate.value.is_object() && candidate.value["Upgrades"].is_array();
        for upgrade in candidate.value["Upgrades"].as_array().into_iter().flatten() {
            let id = upgrade["ItemId"]["$oid"]
                .as_str()
                .or_else(|| upgrade["ItemId"].as_str());
            if let Some(id) = id {
                mods.entry(id)
                    .and_modify(|old| {
                        if *old != Some(upgrade) {
                            *old = None;
                        }
                    })
                    .or_insert(Some(upgrade));
            }
        }
        let groups: Vec<(&str, &Value)> = if let Some(object) = candidate.value.as_object() {
            object
                .iter()
                .filter(|(_, v)| {
                    v.as_array()
                        .is_some_and(|a| a.iter().any(|i| i["Configs"].is_array()))
                })
                .map(|(k, v)| (k.as_str(), v))
                .collect()
        } else {
            vec![("Suits", &candidate.value)]
        };
        for (group, values) in groups {
            for gear in values.as_array().into_iter().flatten().take(4096) {
                for (index, config) in gear["Configs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(12)
                    .enumerate()
                {
                    if !config.is_object() {
                        continue;
                    }
                    let Some(mut item) =
                        configuration_item(gear, config, index, group, &mods, names)
                    else {
                        continue;
                    };
                    item.inventory_resolved = full;
                    if let Ok(key) = serde_json::to_string(&item) {
                        unique.entry(key).or_insert(item);
                    }
                }
            }
        }
        // Моды разлома могут не использоваться ни в одной конфигурации. Они доступны
        // в той же коллекции как самостоятельные записи с привязкой к оружию.
        for upgrade in candidate.value["Upgrades"].as_array().into_iter().flatten() {
            let Some(part) = upgrade_part(upgrade, names)
                .filter(|p| p.fingerprint.is_some() || p.path.contains("/Randomized/"))
            else {
                continue;
            };
            let value = serde_json::json!({"NORMAL":[{"ItemType":part.path, "WeaponUpgrades":[]}]});
            let Some(mut item) = equipment(&value, names).into_iter().next() else {
                continue;
            };
            item.item = part;
            item.category = "Мод разлома".into();
            item.source = "memoryConfiguration".into();
            item.inventory_resolved = true;
            if let Ok(key) = serde_json::to_string(&item) {
                unique.entry(key).or_insert(item);
            }
        }
    }
    let mut items: Vec<_> = unique.into_values().collect();
    items.sort_by(|a, b| {
        (&a.category, &a.item.name, &a.item.path, a.configuration).cmp(&(
            &b.category,
            &b.item.name,
            &b.item.path,
            b.configuration,
        ))
    });
    for (index, item) in items.iter_mut().enumerate() {
        item.key = format!("configuration:{index}");
    }
    items
}

fn match_candidate(
    candidates: &[Candidate],
    bytes: usize,
    same_size_members: usize,
) -> Result<Option<&Candidate>, ()> {
    if same_size_members != 1 {
        return Err(());
    }
    let matches: Vec<_> = candidates.iter().filter(|c| c.bytes == bytes).collect();
    match matches.as_slice() {
        [] => Ok(None),
        [single] => Ok(Some(*single)),
        _ => Err(()),
    }
}

pub(crate) fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let task_app = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || poll(&task_app)).await;
        }
    });
}

fn poll(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pid = find_wf_pid();
    let Ok(mut service) = state.squad.lock() else {
        return;
    };
    if service.pid.is_some() && service.pid != pid {
        service.reset();
    }
    service.pid = pid;
    let Some(pid) = pid else {
        return;
    };
    if !service.enabled || !service.pending || !service.log_ready || service.members.is_empty() {
        return;
    }
    let requests: Vec<_> = service
        .parser
        .peers()
        .iter()
        .filter(|p| Some(p.name.as_str()) != service.parser.local_user())
        .filter_map(|p| p.loadout_bytes.map(|b| (p.name.clone(), b)))
        .collect();
    if requests.is_empty() {
        service.pending = false;
        return;
    }
    service.scanning = true;
    service.pending = false;
    service.attempts += 1;
    let revision = service.revision;
    drop(service);
    let result = capture(pid);
    let names = Names(game_names::get(&state));
    let Ok(mut service) = state.squad.lock() else {
        return;
    };
    service.scanning = false;
    if service.revision != revision || !service.enabled {
        return;
    }
    let candidates = match result {
        Ok(c) => c,
        Err(error) => {
            service.error = Some(error.to_string());
            return;
        }
    };
    service.error = None;
    let mut missing = false;
    for member in &mut service.members {
        let Some((_, bytes)) = requests.iter().find(|(name, _)| name == &member.name) else {
            continue;
        };
        match match_candidate(
            &candidates,
            *bytes,
            requests.iter().filter(|(_, b)| b == bytes).count(),
        ) {
            Ok(Some(candidate)) => {
                member.equipment = equipment(&candidate.value, &names);
                member.captured_at = Some(Utc::now().to_rfc3339());
                member.mastery_rank = candidate.value["PlayerLevel"].as_u64();
                member.status = if member.equipment.is_empty() {
                    "unavailable"
                } else {
                    "matched"
                }
                .into();
            }
            Ok(None) => {
                member.status = "unavailable".into();
                member.equipment.clear();
                member.captured_at = None;
                missing = true;
            }
            Err(()) => {
                member.status = "ambiguous".into();
                member.equipment.clear();
                member.captured_at = None;
            }
        }
    }
    service.pending |= missing && service.attempts < 3;
}

fn view(state: &AppState) -> Result<View, String> {
    let saved = state
        .database
        .lock()
        .map_err(|e| e.to_string())?
        .get_setting::<Vec<SavedBuild>>(SAVED_KEY)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let service = state.squad.lock().map_err(|e| e.to_string())?;
    let mut result = View {
        enabled: service.enabled,
        running: service.pid.is_some(),
        scanning: service.scanning,
        error: service.error.clone(),
        members: if service.pid.is_some() {
            service.members.clone()
        } else {
            Vec::new()
        },
        saved,
        configurations: service.configurations.clone(),
        configurations_at: service.configurations_at.clone(),
        reading_configurations: service.reading_configurations,
    };
    drop(service);
    let names = Names(game_names::get(state));
    for member in &mut result.members {
        for equipment in &mut member.equipment {
            names.localize_equipment(equipment);
        }
    }
    for equipment in &mut result.configurations {
        names.localize_equipment(equipment);
    }
    for build in &mut result.saved {
        names.localize_equipment(&mut build.equipment);
    }
    Ok(result)
}

#[tauri::command]
pub(crate) async fn squad_read_configurations(state: State<'_, AppState>) -> Result<View, String> {
    let pid = find_wf_pid().ok_or("Запустите Warframe и откройте Арсенал.")?;
    let revision = {
        let mut service = state.squad.lock().map_err(|e| e.to_string())?;
        if service.reading_configurations {
            return Err("Чтение конфигураций уже выполняется.".into());
        }
        service.reading_configurations = true;
        service.configurations.clear();
        service.configurations_at = None;
        service.configurations_revision += 1;
        service.configurations_revision
    };
    let result = tauri::async_runtime::spawn_blocking(move || capture_suits(pid)).await;
    let names = Names(game_names::get(&state));
    let result = result
        .map_err(|_| "Не удалось завершить чтение конфигураций.".to_owned())
        .and_then(|r| r.map_err(|e| e.to_string()));
    {
        let mut service = state.squad.lock().map_err(|e| e.to_string())?;
        service.reading_configurations = false;
        if service.configurations_revision != revision || find_wf_pid() != Some(pid) {
            return Err("Состояние игры изменилось. Прочитайте конфигурации снова.".into());
        }
        service.configurations = configurations(&result?, &names);
        service.configurations_at = Some(Utc::now().to_rfc3339());
    }
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_status(state: State<'_, AppState>) -> Result<View, String> {
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_set_enabled(enabled: bool, state: State<'_, AppState>) -> Result<View, String> {
    state
        .database
        .lock()
        .map_err(|e| e.to_string())?
        .set_setting(ENABLED_KEY, &enabled)
        .map_err(|e| e.to_string())?;
    {
        let mut service = state.squad.lock().map_err(|e| e.to_string())?;
        service.enabled = enabled;
        service.pending = enabled;
        service.attempts = 0;
        service.revision += 1;
    }
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_refresh(state: State<'_, AppState>) -> Result<View, String> {
    {
        let mut service = state.squad.lock().map_err(|e| e.to_string())?;
        service.pending = true;
        service.attempts = 0;
        service.error = None;
        service.revision += 1;
    }
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_save_build(
    player: String,
    equipment_key: String,
    captured_at: String,
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<View, String> {
    let mut build = {
        let service = state.squad.lock().map_err(|e| e.to_string())?;
        let (equipment, platform, snapshot, player) = if source.as_deref() == Some("configuration")
        {
            if service.configurations_at.as_deref() != Some(captured_at.as_str()) {
                return Err("Конфигурации изменились. Просмотрите их и сохраните снова.".into());
            }
            let item = service
                .configurations
                .iter()
                .find(|e| e.key == equipment_key)
                .ok_or("Конфигурация уже недоступна.")?
                .clone();
            (
                item,
                String::new(),
                captured_at,
                "Владелец не подтверждён".to_owned(),
            )
        } else {
            let member = service
                .members
                .iter()
                .find(|m| m.name == player && m.status == "matched")
                .ok_or("Экипировка уже недоступна. Обновите отряд.")?;
            if member.captured_at.as_deref() != Some(captured_at.as_str()) {
                return Err(
                    "Снимок экипировки изменился. Просмотрите его и сохраните снова.".into(),
                );
            }
            let equipment = member
                .equipment
                .iter()
                .find(|e| e.key == equipment_key)
                .ok_or("Предмет уже недоступен.")?
                .clone();
            (
                equipment,
                member.platform.clone(),
                member.captured_at.clone().unwrap_or_default(),
                player,
            )
        };
        SavedBuild {
            id: String::new(),
            title: if equipment.item.name.is_empty() {
                equipment.category.clone()
            } else {
                equipment.item.name.clone()
            },
            player,
            platform,
            captured_at: snapshot,
            saved_at: Utc::now().to_rfc3339(),
            note: String::new(),
            equipment,
        }
    };
    {
        let db = state.database.lock().map_err(|e| e.to_string())?;
        let mut saved = db
            .get_setting::<Vec<SavedBuild>>(SAVED_KEY)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let exists = saved.iter().any(|b| {
            b.player == build.player
                && b.captured_at == build.captured_at
                && b.equipment.key == build.equipment.key
        });
        if !exists {
            if saved.len() >= MAX_SAVED {
                return Err(
                    "Сохранено 200 билдов. Удалите ненужные перед добавлением нового.".into(),
                );
            }
            build.id = format!(
                "{}-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                saved.len()
            );
            saved.insert(0, build);
            db.set_setting(SAVED_KEY, &saved)
                .map_err(|e| e.to_string())?;
        }
    }
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_edit_build(
    id: String,
    title: String,
    note: String,
    state: State<'_, AppState>,
) -> Result<View, String> {
    if title.trim().is_empty() || title.chars().count() > 100 || note.chars().count() > 3000 {
        return Err("Название: 1–100 символов. Заметка: до 3000 символов.".into());
    }
    {
        let db = state.database.lock().map_err(|e| e.to_string())?;
        let mut saved = db
            .get_setting::<Vec<SavedBuild>>(SAVED_KEY)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let build = saved
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or("Билд уже удалён.")?;
        build.title = title.trim().into();
        build.note = note.trim().into();
        db.set_setting(SAVED_KEY, &saved)
            .map_err(|e| e.to_string())?;
    }
    view(&state)
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn squad_delete_build(id: String, state: State<'_, AppState>) -> Result<View, String> {
    {
        let db = state.database.lock().map_err(|e| e.to_string())?;
        let mut saved = db
            .get_setting::<Vec<SavedBuild>>(SAVED_KEY)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        saved.retain(|b| b.id != id);
        db.set_setting(SAVED_KEY, &saved)
            .map_err(|e| e.to_string())?;
    }
    view(&state)
}

pub(crate) fn service(enabled: bool) -> Mutex<Service> {
    Mutex::new(Service {
        enabled,
        ..Service::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn saved_equipment_is_relocalized_without_losing_build_data() {
        let names = Names(Arc::new(game_names::test_names()));
        let original = json!({"key":"old", "category":"Варфрейм", "item":{"path":"/Lotus/StoreItems/Test", "name":"Test", "nameEn":"Test", "kind":"equipment", "rank":null},
            "level":30,"forma":4,"upgrades":[],"modularParts":[],"unreadableUpgrades":0,
            "shards":[{"color":"ACC_RED","effect":{"path":"/Lotus/Test","name":"Test","nameEn":"Test","kind":"unknown","rank":7}}],
            "abilityOverride":{"slot":4,"ability":{"path":"/Lotus/Powers/RhinoRoarAbility","name":"","nameEn":"","kind":"unknown","rank":null}}});
        let mut equipment: Equipment = serde_json::from_value(original).unwrap();
        names.localize_equipment(&mut equipment);
        assert_eq!(equipment.item.name, "Чертёж");
        assert_eq!(equipment.item.name_en, "Test");
        assert_eq!(equipment.forma, Some(4));
        let shard = &equipment.shards.unwrap()[0];
        assert_eq!(shard.effect.name, "Чертёж");
        assert_eq!(shard.effect.rank, Some(7));
        assert_eq!(equipment.ability_override.unwrap().ability.name, "Рёв");
    }
    #[test]
    fn inventory_resolves_reused_references_and_keeps_unknown_positions_private() {
        let inventory = json!({
            "AccountId":"private-account", "Upgrades":[
                {"ItemId":{"$oid":"private-mod"},"ItemType":"/Lotus/Upgrades/Mods/Strength","UpgradeFingerprint":"{\"lvl\":7}"},
                {"ItemId":{"$oid":"private-missing-rank"},"ItemType":"/Lotus/Upgrades/Mods/Duration","UpgradeFingerprint":"broken"}
            ], "Suits":[{"ItemType":"/Lotus/Powersuits/Test","Configs":[{"Upgrades":["private-mod","", "private-unknown", "private-missing-rank", null]},{"Upgrades":["private-mod"]}]}],
            "LongGuns":[{"ItemType":"/Lotus/Weapons/Rifle","Configs":[{"Upgrades":["private-mod","/Lotus/Upgrades/Mods/Direct"]}]}]
        });
        let candidates = [Candidate {
            bytes: 1,
            copies: 1,
            value: inventory,
        }];
        let items = configurations(&candidates, &Names::default());
        assert_eq!(items.len(), 3);
        let frame = items
            .iter()
            .find(|i| i.category == "Варфрейм" && i.configuration == Some(1))
            .unwrap();
        assert_eq!(frame.upgrades[0].rank, Some(7));
        assert_eq!(frame.upgrades[1].rank, None);
        assert_eq!(frame.upgrades[1].slot_index, Some(3));
        let slots = frame.upgrade_slots.as_ref().unwrap();
        assert_eq!(
            slots.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(),
            ["resolved", "empty", "unresolved", "resolved", "unresolved"]
        );
        assert_eq!(frame.unreadable_upgrades, 2);
        assert!(items.iter().all(|i| i.inventory_resolved));
        let rifle = items
            .iter()
            .find(|i| i.category == "Основное оружие")
            .unwrap();
        assert_eq!(rifle.upgrades[0].rank, Some(7));
        assert_eq!(rifle.upgrades[1].rank, None);
        assert!(!serde_json::to_string(&items).unwrap().contains("private"));
    }

    #[test]
    fn conflicting_mod_ids_are_not_silently_resolved_and_copies_are_not_merged() {
        let candidate = |mod_type: &str| Candidate {
            bytes: 1,
            copies: 1,
            value: json!({
                "Upgrades":[{"ItemId":{"$oid":"same"},"ItemType":mod_type}],
                "Suits":[{"ItemType":"/Lotus/Powersuits/Test","Configs":[{"Upgrades":["same"]}]}]
            }),
        };
        let a = candidate("/Lotus/Upgrades/Mods/A");
        let mut b = candidate("/Lotus/Upgrades/Mods/B");
        b.value["Upgrades"]
            .as_array_mut()
            .unwrap()
            .push(json!({"ItemId":{"$oid":"same"},"ItemType":"/Lotus/Upgrades/Mods/C"}));
        let items = configurations(&[a, b], &Names::default());
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.unreadable_upgrades == 1));
        assert!(
            items
                .iter()
                .any(|i| i.upgrades.first().is_some_and(|p| p.path.ends_with("/A")))
        );
    }

    #[test]
    fn riven_fingerprint_is_associated_with_its_weapon_and_survives_storage() {
        let value = json!({"Suits":[],"Upgrades":[{"ItemId":{"$oid":"private-riven"},"ItemType":"/Lotus/Upgrades/Mods/Randomized/Test",
            "UpgradeFingerprint":json!({"compat":"/Lotus/Weapons/Guandao","lvl":8,"lvlReq":12,"rerolls":68,"pol":"AP_TACTIC","account":"private-account","buffs":[{"Tag":"WeaponCritDamageMod","Value":912_519_347}],"curses":[{"Tag":"ComboDurationMod","Value":317_116_412}]}).to_string()}]});
        let items = configurations(
            &[Candidate {
                bytes: 1,
                copies: 1,
                value,
            }],
            &Names::default(),
        );
        assert_eq!(items.len(), 1);
        let equipment = items[0].clone();
        assert_eq!(equipment.category, "Мод разлома");
        let f = equipment.item.fingerprint.as_ref().unwrap();
        assert_eq!(f.weapon_path.as_deref(), Some("/Lotus/Weapons/Guandao"));
        assert_eq!(f.rerolls, Some(68));
        assert_eq!(f.buffs[0].value, Some(912_519_347.0));
        let serialized = serde_json::to_string(&equipment).unwrap();
        assert!(!serialized.contains("private"));
        let restored: Equipment = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            restored.item.fingerprint.unwrap().curses[0].tag,
            "ComboDurationMod"
        );
        let db = platscope_storage::Database::open_in_memory().unwrap();
        let saved = SavedBuild {
            id: "test-riven".into(),
            title: "Мод разлома".into(),
            player: "Владелец не подтверждён".into(),
            platform: String::new(),
            captured_at: "2026-09-12T00:00:00Z".into(),
            saved_at: "2026-09-12T00:00:00Z".into(),
            note: String::new(),
            equipment,
        };
        db.set_setting(SAVED_KEY, &vec![saved]).unwrap();
        let stored = db
            .get_setting::<Vec<SavedBuild>>(SAVED_KEY)
            .unwrap()
            .unwrap();
        assert_eq!(
            stored[0]
                .equipment
                .item
                .fingerprint
                .as_ref()
                .unwrap()
                .rerolls,
            Some(68)
        );
        let part = upgrade_part(&json!({"ItemType":"/Lotus/Upgrades/Mods/Randomized/Test","UpgradeFingerprint":"{\"compat\":\"/Lotus/Weapons/Other\"}"}),&Names::default()).unwrap();
        assert_eq!(part.rank, None);
        assert_eq!(part.fingerprint.unwrap().rerolls, None);
    }
    #[test]
    fn damaged_uninstalled_riven_remains_visible_without_invented_properties() {
        let items = configurations(
            &[Candidate {
                bytes: 1,
                copies: 1,
                value: json!({"Suits":[], "Upgrades":[{
                    "ItemId":{"$oid":"private-riven"},
                    "ItemType":"/Lotus/Upgrades/Mods/Randomized/Test",
                    "UpgradeFingerprint":"{broken"
                }]}),
            }],
            &Names::default(),
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "Мод разлома");
        assert!(items[0].item.fingerprint.is_none());
        assert!(items[0].item.rank.is_none());
        assert!(!serde_json::to_string(&items).unwrap().contains("private"));
    }

    #[test]
    fn preserves_shards_and_configuration_links_without_assigning_an_owner() {
        let suit = json!([{"ItemType":"/Lotus/Powersuits/Wisp/WispPrime", "ItemId":"private-id", "ArchonCrystalUpgrades":[{"Color":"ACC_RED", "UpgradeType":"/Lotus/Upgrades/Invigorations/ArchonCrystalUpgrades/ArchonCrystalUpgradeWarframeAbilityStrength"}], "Configs":[{"Name":"private-name", "AbilityOverride":{"Ability":"/Lotus/Powersuits/Rhino/Abilities/RhinoRoarAbility", "Index":3}, "Upgrades":["private-instance", "/Lotus/Upgrades/Mods/Test"]}, {"Upgrades":[]}]}]);
        let candidate = Candidate {
            bytes: 100,
            copies: 1,
            value: suit,
        };
        let result = configurations(&[candidate], &Names::default());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].configuration, Some(1));
        assert_eq!(result[0].ability_override.as_ref().unwrap().slot, Some(4));
        assert_eq!(result[0].shards.as_ref().unwrap().len(), 1);
        assert!(result[1].ability_override.is_none());
        assert_eq!(result[0].source, "memoryConfiguration");
        assert_eq!(result[0].unreadable_upgrades, 1);
        assert!(!serde_json::to_string(&result).unwrap().contains("private"));
        let peer = equipment(
            &json!({"NORMAL":[{"ItemType":"/Lotus/Powersuits/Wisp/WispPrime"}]}),
            &Names::default(),
        );
        assert!(peer[0].shards.is_none());
        assert!(peer[0].ability_override.is_none());
        assert_eq!(peer[0].source, "squad");
    }

    #[test]
    fn legacy_builds_and_missing_details_do_not_become_empty_or_default_bonuses() {
        let legacy = json!({"key":"NORMAL:0", "category":"Варфрейм", "item":{"path":"/Lotus/Test", "name":"Тест", "nameEn":"Test", "kind":"equipment", "rank":null}, "level":30,"forma":null,"upgrades":[],"modularParts":[],"unreadableUpgrades":0});
        let item: Equipment = serde_json::from_value(legacy).unwrap();
        assert!(item.shards.is_none());
        assert!(item.ability_override.is_none());
        assert!(shards(&json!([]), &Names::default()).unwrap().is_empty());
        assert!(shards(&json!([{"Color":"ACC_RED"}]), &Names::default()).is_none());
        let ability = ability_override(
            &json!({"Ability":"/Lotus/Test", "Index":99}),
            &Names::default(),
        )
        .unwrap();
        assert!(ability.slot.is_none());
    }

    #[test]
    #[ignore = "Ручная проверка требует запущенного Warframe"]
    fn live_configuration_probe() {
        let pid = find_wf_pid().unwrap();
        let candidates = capture_suits(pid).unwrap();
        let items = configurations(&candidates, &Names::default());
        let with_shards = items
            .iter()
            .filter(|i| i.shards.as_ref().is_some_and(|s| !s.is_empty()))
            .count();
        let with_ability = items
            .iter()
            .filter(|i| i.ability_override.is_some())
            .count();
        println!(
            "Конфигураций: {}; с осколками: {with_shards}; с заменой: {with_ability}",
            items.len()
        );
        assert!(!items.is_empty());
        let directory = std::path::Path::new("../../../target");
        std::fs::write(
            directory.join("memory-configurations-preview.json"),
            serde_json::to_vec_pretty(&items).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn exports_utf8_text_and_rejects_unbounded_payloads() {
        let directory = std::env::temp_dir().join(format!(
            "platscope-build-export-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let content = "Билд\nПоток Прайм — ранг неизвестен\n";
        let path = export_text(&directory, content).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        assert!(export_text(&directory, &"x".repeat(256 * 1024 + 1)).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn ambiguous_sizes_are_never_assigned_to_a_player() {
        let one = Candidate {
            bytes: 100,
            copies: 20,
            value: json!({}),
        };
        let two = Candidate {
            bytes: 100,
            copies: 1,
            value: json!({}),
        };
        assert!(match_candidate(&[one, two], 100, 1).is_err());
        assert!(match_candidate(&[], 100, 2).is_err());
        assert!(match_candidate(&[], 100, 1).unwrap().is_none());
    }
    #[test]
    fn preserves_unknown_upgrades_without_inventing_ranks_or_slots() {
        let names = Names::default();
        let value = json!({"NORMAL":[{"ItemType":"/Lotus/Suits/Test","Level":30,"WeaponUpgrades":["", "/Lotus/Upgrades/Mods/Test", "/Lotus/UnknownUpgrade", {"unexpected":1}]}],"AccountId":"secret"});
        let items = equipment(&value, &names);
        assert_eq!(items[0].upgrades.len(), 2);
        assert!(items[0].upgrades[0].rank.is_none());
        assert_eq!(items[0].upgrades[1].kind, "unknown");
        assert_eq!(items[0].unreadable_upgrades, 1);
        assert!(!serde_json::to_string(&items).unwrap().contains("secret"));
    }
    #[test]
    fn split_log_lines_and_departures_do_not_keep_a_members_build() {
        let mut s = Service::default();
        s.feed("1 Net [Info]: AddSquadMember: Guest, mm=abc, squad");
        s.feed("Count=2\n");
        assert_eq!(s.members.len(), 1);
        s.feed("2 Net [Info]: RemoveSquadMember: Guest has been removed\n");
        assert!(s.members.is_empty());
    }

    #[test]
    fn changed_loadout_size_invalidates_the_previous_snapshot() {
        let mut s = Service::default();
        s.feed("1 Net [Info]: AddSquadMember: Guest, mm=abc, squadCount=2\n2 Net [Info]: MatchingServiceWeb::ProcessSquadMessage received JOIN message from Guest, loadout: 100 bytes\n");
        s.members[0].status = "matched".into();
        s.members[0].captured_at = Some("old".into());
        s.feed("3 Net [Info]: MatchingServiceWeb::ProcessSquadMessage received JOIN message from Guest, loadout: 101 bytes\n");
        assert_eq!(s.members[0].status, "waiting");
        assert!(s.members[0].captured_at.is_none());
    }

    #[test]
    fn saved_build_is_independent_of_roster_and_preserves_unknown_fields_in_storage() {
        let db = platscope_storage::Database::open_in_memory().unwrap();
        let value =
            json!({"NORMAL":[{"ItemType":"/Lotus/Test","WeaponUpgrades":["/Lotus/Unknown"]}]});
        let equipment = equipment(&value, &Names::default()).remove(0);
        let build = SavedBuild {
            id: "test".into(),
            title: "Идея".into(),
            player: "Guest".into(),
            platform: "PC".into(),
            captured_at: "time".into(),
            saved_at: "time".into(),
            note: "Уточнить ранги".into(),
            equipment,
        };
        db.set_setting(SAVED_KEY, &vec![build]).unwrap();
        let saved: Vec<SavedBuild> = db.get_setting(SAVED_KEY).unwrap().unwrap();
        assert_eq!(saved[0].note, "Уточнить ранги");
        assert_eq!(saved[0].equipment.upgrades[0].path, "/Lotus/Unknown");
        assert!(saved[0].equipment.upgrades[0].rank.is_none());
    }
}
