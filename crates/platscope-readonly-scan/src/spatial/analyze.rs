use super::*;
use super::{
    geometry,
    profile::{Profile, ProfilePack},
    source::{LiveMemory, q, u64_at},
    types::Decoder,
};
use chrono::Utc;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const MAX_SCAN: u64 = 24 * 1024 * 1024 * 1024;
const MAX_TIME: Duration = Duration::from_secs(300);
pub(super) const MAX_OBJECTS: usize = 20_000;
fn detail(label: &str, value: impl Into<String>) -> Detail {
    Detail {
        label: label.into(),
        value: value.into(),
    }
}
fn stamp() -> String {
    Utc::now().to_rfc3339()
}
fn handle(m: &mut dyn Memory, address: u64) -> Result<(u64, u64)> {
    let h = q(m, address)?;
    if h == 0 {
        return Err("Нет ссылки".into());
    }
    let native = q(m, h)?;
    if native == 0 || q(m, native + 0x10)? != h {
        return Err("Не подтверждена обратная ссылка".into());
    }
    Ok((h, native))
}
fn identity(m: &mut dyn Memory, a: u64, expected: u64) -> Result<Identity> {
    let b = m.read(a, 24)?;
    let vtable = u64_at(&b, 0)?;
    let metadata = u64_at(&b, 8)?;
    let h = u64_at(&b, 16)?;
    if metadata > 0x0000_7fff_ffef_ffff || vtable != expected || h == 0 || q(m, h)? != a {
        return Err("Объект не подтверждён".into());
    }
    Ok(Identity {
        address: a,
        vtable,
        metadata,
        handle: h,
        zone: None,
        item: None,
        action: None,
        item_offset: 0x528,
        moving: false,
        decree_fragment: false,
        missed_reads: 0,
    })
}
fn short_resource(s: &str) -> String {
    s.rsplit('/')
        .next()
        .unwrap_or(s)
        .trim_matches('"')
        .to_owned()
}
fn pickup_label(item: &str) -> (String, String) {
    match short_resource(item).as_str() {
        "ZarimanDogTagCommon" => ("Перо Бездны: обычное".into(), "Voidplume Down".into()),
        "ZarimanDogTagUncommon" => ("Перо Бездны: необычное".into(), "Voidplume Vane".into()),
        "ZarimanDogTagRare" => ("Перо Бездны: редкое".into(), "Voidplume Crest".into()),
        "ZarimanDogTagBounty" => ("Перо Бездны: за заказ".into(), "Voidplume Quill".into()),
        _ => ("Предмет на уровне".into(), short_resource(item)),
    }
}
fn character_kind(family: &str, tag: Option<&str>, loc_tag: Option<&str>) -> &'static str {
    if family == "avatar" {
        "avatar"
    } else if tag == Some("Hostage") || loc_tag == Some("/Lotus/Language/Game/Hostage") {
        "hostage"
    } else {
        "npc"
    }
}
pub(super) fn local_mission_root(m: &mut dyn Memory, scene: &Scene) -> Option<u64> {
    let locals: Vec<_> = scene.players.iter().filter(|p| p.local).collect();
    let [local] = locals.as_slice() else {
        return None;
    };
    let avatar = u64::from_str_radix(local.avatar_key.trim_start_matches("0x"), 16).ok()?;
    q(m, avatar + 0x1e0).ok().filter(|root| *root != 0)
}
fn decree_fragment_anchor(m: &mut dyn Memory, address: u64, action: u64) -> Result<[f32; 3]> {
    if handle(m, address + 0x488)?.1 != action {
        return Err("Действие фрагмента изменилось".into());
    }
    let root = q(m, address + 0x1e0)?;
    if root == 0 || q(m, action + 0x1e0)? != root {
        return Err("Действие фрагмента в другом контексте".into());
    }
    let marker = geometry::decree_fragment_position(m, address)?;
    let action_position = geometry::position(m, action, false)?;
    if marker
        .iter()
        .zip(action_position)
        .any(|(a, b)| (a - b).abs() > 0.02)
    {
        return Err("Действие фрагмента не совпадает с его положением".into());
    }
    Ok(marker)
}
pub(super) fn object(
    m: &mut dyn Memory,
    a: u64,
    family: &str,
    vt: u64,
    decoder: &mut Decoder,
    profile: &Profile,
) -> Result<(SceneObject, Identity)> {
    let mut id = identity(m, a, vt)?;
    let info = decoder.chain(m, id.metadata)?;
    id.moving = matches!(family, "avatar" | "npc");
    id.decree_fragment = family == "decree_fragment";
    let (fragment_action, position) = if id.decree_fragment {
        let (_, action) = handle(m, a + 0x488)?;
        (Some(action), decree_fragment_anchor(m, a, action)?)
    } else {
        (None, geometry::position(m, a, id.moving)?)
    };
    if position.iter().all(|v| v.abs() < 0.00001) {
        return Err("Нулевой шаблон".into());
    }
    let mut details = vec![detail(
        "Источник",
        "Объект в памяти; доступность взаимодействия не установлена",
    )];
    let mut item_path = None;
    let mut availability = "unknown";
    let mut variant_key = None;
    let (kind, label, name_en) = match family {
        "dragon_door" => {
            if !info.names.iter().any(|n| n == "ContextAction *") {
                return Err("Не подтверждён тип действия двери".into());
            }
            let (_, zone) = handle(m, a + 0x428)?;
            let zone_meta = q(m, zone + 8)?;
            if !decoder
                .chain(m, zone_meta)?
                .names
                .iter()
                .any(|n| n == "Zone *")
            {
                return Err("Не подтверждена зона двери".into());
            }
            let root = q(m, a + 0x1e0)?;
            if root == 0 || q(m, zone + 0x1e0)? != root {
                return Err("Дверь не принадлежит зоне миссии".into());
            }
            let (key, item) = super::dragon_door::required_key(m, a, decoder)?;
            id.zone = Some(zone);
            id.item = Some(item);
            id.item_offset = super::dragon_door::REQUIRED_ITEM;
            variant_key = Some(format!("dragon-door-v1:{}", key.code));
            details.push(detail(
                "Требуемый ключ",
                format!("Ключ Дракона: {} · {}", key.label, key.english),
            ));
            details.push(detail(
                "Состояние двери",
                "Открытие двери и наличие ключа у игрока не проверены",
            ));
            (
                "dragon_door",
                format!("Дверь Дракона · {}", key.label),
                format!("Orokin Vault · {}", key.english),
            )
        }
        "pickup" | "decree_fragment" => {
            let decree_fragment = family == "decree_fragment";
            if decree_fragment
                && (!info.names.iter().any(|n| n == "PickUp *")
                    || info.field("Tag").as_deref() != Some("DuviriEndlessPickUp")
                    || info.field("PickUpItemType").as_deref() != Some("DuviriArenaBoonItem")
                    || info.field("PickUpActionType").as_deref()
                        != Some("DuviriArenaBoonFragmentAutoPickUp")
                    || info.field("Mesh").as_deref()
                        != Some("/Lotus/Fx/Gameplay/HoverBoard/DuviriArenaBoonPickup.fbx")
                    || info.field("Script").as_deref()
                        != Some("/Lotus/Scripts/Duviri/Gameplay/DuviriEndless.lua"))
            {
                return Err("Не подтверждён фрагмент декрета".into());
            }
            let (_, zone) = handle(m, a + 0x428)?;
            let zone_meta = q(m, zone + 8)?;
            let zi = decoder.chain(m, zone_meta)?;
            if !zi.names.iter().any(|n| n == "Zone *") {
                return Err("Не подтверждена зона предмета".into());
            }
            let root = if decree_fragment {
                let root = q(m, a + 0x1e0)?;
                if root == 0 || q(m, zone + 0x1e0)? != root {
                    return Err("Фрагмент не принадлежит зоне миссии".into());
                }
                Some(root)
            } else {
                None
            };
            let (_, item) = handle(m, a + 0x528)?;
            let item_meta = q(m, item + 8)?;
            let it = decoder.chain(m, item_meta)?;
            if !it.names.iter().any(|n| n == "Item *") {
                return Err("Не подтверждён экземпляр предмета".into());
            }
            if decree_fragment && !it.names.iter().any(|n| n == "LotusPickUpItem *") {
                return Err("Не подтверждён предмет фрагмента".into());
            }
            id.zone = Some(zone);
            id.item = Some(item);
            let path = info.field("PickUpItemType").ok_or("Нет типа предмета")?;
            if decree_fragment {
                let action = fragment_action.unwrap();
                let action_meta = q(m, action + 8)?;
                let action_info = decoder.chain(m, action_meta)?;
                if !action_info.names.iter().any(|n| n == "PickUpAction *")
                    || action_info.field("ActionSound").as_deref()
                        != Some("/Lotus/Sounds/UI/Pickups/DuviriBoonFragmentPickup")
                    || q(m, action + 0x1e0)? != root.unwrap()
                {
                    return Err("Не подтверждено действие фрагмента".into());
                }
                details.push(detail("Зона", format!("0x{zone:x}")));
                details.push(detail("Тип предмета", path.clone()));
                details.push(detail("Ресурс", info.field("Mesh").unwrap()));
                details.push(detail(
                    "Действие подбора",
                    "Найдено; возможность подобрать ещё не подтверждена",
                ));
                id.action = Some(action);
                item_path = Some(path);
                variant_key = Some("decree-fragment-v1".into());
                (
                    "decree_fragment",
                    "Фрагмент декрета".into(),
                    "Decree Fragment".into(),
                )
            } else {
                let feather = short_resource(&path).starts_with("ZarimanDogTag");
                if feather {
                    let loc = it.field("LocalizeTag").ok_or("Нет имени пера")?;
                    if !loc.ends_with(&format!("{}Name", short_resource(&path))) {
                        return Err("Тип пера не совпадает с экземпляром предмета".into());
                    }
                }
                match handle(m, a + 0x488) {
                    Ok((_, action)) => {
                        let action_meta = q(m, action + 8)?;
                        match decoder.chain(m, action_meta) {
                            Ok(t) if t.names.iter().any(|n| n == "PickUpAction *") => {
                                details.push(detail(
                                    "Действие подбора",
                                    "Найдено; возможность подобрать ещё не подтверждена",
                                ))
                            }
                            _ => details.push(detail(
                                "Действие подбора",
                                "Ссылка изменилась или не подтверждена",
                            )),
                        }
                    }
                    Err(_) => details.push(detail(
                        "Действие подбора",
                        "Не подтверждено; это не означает, что предмет уже подобран",
                    )),
                }
                details.push(detail("Зона", format!("0x{zone:x}")));
                details.push(detail("Тип предмета", path.clone()));
                let (label, en) = pickup_label(&path);
                item_path = Some(path);
                (if feather { "feather" } else { "pickup" }, label, en)
            }
        }
        "avatar" => {
            super::context::owner(m, a, profile)?;
            (
                "avatar",
                "Варфрейм или оператор".into(),
                "Warframe / Operator".into(),
            )
        }
        "extraction" | "waypoint" | "terminal" => {
            let (_, zone) = handle(m, a + 0x428)?;
            let meta = q(m, zone + 8)?;
            if !decoder.chain(m, meta)?.names.iter().any(|n| n == "Zone *") {
                return Err("Не подтверждена зона действия".into());
            }
            id.zone = Some(zone);
            let (kind, label, name) = interaction(family, &info)?;
            details.push(detail("Ресурс", name));
            if kind == "lootspot" {
                details.push(detail(
                    "Назначение",
                    "Возможное место появления; наличие предмета не подтверждено",
                ));
            }
            (kind, label.into(), name.into())
        }
        "npc" => {
            let kind = character_kind(
                family,
                info.field("Tag").as_deref(),
                info.field("LocTag").as_deref(),
            );
            (
                kind,
                if kind == "hostage" {
                    "Заложник"
                } else {
                    "NPC"
                }
                .into(),
                info.field("Mesh")
                    .map(|s| short_resource(&s))
                    .unwrap_or("LotusNpcAvatar".into()),
            )
        }
        "spawnpoint" => (
            "spawnpoint",
            "Точка появления персонажа".into(),
            "NpcSpawnPoint".into(),
        ),
        "decoration" | "effect" => {
            let mesh = info
                .field("Mesh")
                .or_else(|| info.field("Material"))
                .unwrap_or_default();
            if mesh.is_empty() {
                return Err("Декорация без подтверждённого ресурса".into());
            }
            // У детали должен быть проверяемый владелец зоны, иначе это может быть ресурсный образец.
            let (_, zone) = handle(m, a + 0x428)?;
            let zone_meta = q(m, zone + 8)?;
            let z = decoder.chain(m, zone_meta)?;
            if !z.names.iter().any(|n| n == "Zone *") {
                return Err("Декорация без зоны".into());
            }
            id.zone = Some(zone);
            let (kind, label) = if family == "decoration" && super::cache::is_grineer_cache(&info) {
                availability = super::cache::state(m, a, decoder, profile).unwrap_or("unknown");
                ("cache", "Тайник Гринир")
            } else if mesh.contains("LootLockerIcon") {
                ("locker", "Обозначение шкафчика")
            } else if mesh.contains("ConsoleDoorPanel") {
                ("panel", "Панель управления")
            } else {
                ("decoration", "Деталь окружения")
            };
            details.push(detail("Ресурс", mesh.clone()));
            (
                kind,
                label.into(),
                if kind == "cache" {
                    "Grineer Resource Cache".into()
                } else {
                    short_resource(&mesh)
                },
            )
        }
        _ => return Err("Неизвестное семейство".into()),
    };
    if !matches!(
        family,
        "pickup" | "decree_fragment" | "decoration" | "effect"
    ) {
        if let Ok((_, zone)) = handle(m, a + 0x428) {
            id.zone = Some(zone);
        }
        if family == "avatar" {
            details.push(detail(
                "Принадлежность",
                "Подтверждена двусторонней связью с игроком",
            ));
        }
    }
    for (field, label) in [
        ("OverrideMaterial", "Материалы варианта"),
        ("MaterialForSwap", "Материал после изменения"),
        ("ServerChildren", "Дочерние действия варианта"),
        ("CompleteScript", "Действие варианта"),
    ] {
        if let Some(value) = info.block(field) {
            let lines: Vec<_> = value
                .lines()
                .filter(|l| {
                    l.starts_with('/')
                        || l.starts_with("Type=")
                        || l.starts_with("Script=")
                        || l.starts_with("Function=")
                })
                .take(12)
                .collect();
            if !lines.is_empty() {
                details.push(detail(label, lines.join(" · ")));
            }
        }
    }
    Ok((
        SceneObject {
            key: format!("0x{a:x}"),
            kind: kind.into(),
            label,
            name_en,
            item_path,
            variant_key: Some(variant_key.unwrap_or_else(|| info.variant_key())),
            position,
            position_fresh: true,
            type_names: info.names,
            availability: availability.into(),
            details,
        },
        id,
    ))
}
fn interaction(
    family: &str,
    info: &super::types::TypeInfo,
) -> Result<(&'static str, &'static str, &'static str)> {
    let tag = info.field("Tag").unwrap_or_default();
    let has_script = |path: &str| {
        info.properties
            .iter()
            .any(|s| s.lines().any(|l| l == format!("Script={path}")))
    };
    match family {
        "extraction"
            if tag == "ExtractionTrigger" && has_script("/Lotus/Scripts/ExtractionTimer.lua") =>
        {
            Ok(("extraction", "Эвакуация", "ExtractionTrigger"))
        }
        "waypoint" => match tag.as_str() {
            "RareLootCrateWaypoint" => Ok((
                "lootspot",
                "Возможное место редкого контейнера",
                "RareLootCrateWaypoint",
            )),
            "UltraRareLootCrateWaypoint" => Ok((
                "lootspot",
                "Возможное место особо редкого контейнера",
                "UltraRareLootCrateWaypoint",
            )),
            "ScannablePlant" => Ok(("lootspot", "Возможное место растения", "ScannablePlant")),
            "SentientArtifactWaypoint" => Ok((
                "lootspot",
                "Возможное место артефакта",
                "SentientArtifactWaypoint",
            )),
            _ => Err("Неизвестное назначение точки".into()),
        },
        "terminal" if has_script("/Lotus/Scripts/BipedSpawner.lua") => {
            Ok(("terminal", "Терминал союзного МОА", "BipedSpawner"))
        }
        "terminal" if has_script("/Lotus/Scripts/Rescue.lua") => {
            Ok(("terminal", "Терминал спасения", "RescuePanicButton"))
        }
        "terminal" if has_script("/Lotus/Scripts/PanicButton.lua") => {
            Ok(("terminal", "Терминал безопасности", "PanicButton"))
        }
        // Остальные действия не получают выдуманного назначения по одному базовому классу.
        _ => Err("Назначение действия не подтверждено".into()),
    }
}
#[derive(Debug)]
pub enum AnalysisFailure {
    UnsupportedBuild(String),
    Other(String),
}

impl From<String> for AnalysisFailure {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for AnalysisFailure {
    fn from(message: &str) -> Self {
        Self::Other(message.to_owned())
    }
}

impl std::fmt::Display for AnalysisFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedBuild(message) | Self::Other(message) => f.write_str(message),
        }
    }
}

fn run(
    m: &mut dyn Memory,
    pack: &ProfilePack,
    source: &str,
    started_at: String,
    input_complete: bool,
    cancel: &AtomicBool,
    progress: &impl Fn(AnalysisProgress),
) -> std::result::Result<Scene, AnalysisFailure> {
    cancelled(cancel)?;
    progress(AnalysisProgress {
        stage: "Проверка версии игры".into(),
        ..Default::default()
    });
    let profile = Profile::validate_with_pack(m, pack).map_err(|error| {
        if error == "В источнике нет Warframe.x64.exe" || error == "Некорректная граница модуля"
        {
            AnalysisFailure::Other(error)
        } else {
            AnalysisFailure::UnsupportedBuild(error)
        }
    })?;
    let mut decoder = Decoder::new(profile.clone())?;
    let targets = profile.targets()?;
    let ranges = m.ranges();
    let total_bytes = ranges.iter().map(|r| r.length as u64).sum();
    let started = Instant::now();
    let mut scanned = 0;
    let mut candidates: BTreeMap<u64, (u64, &str)> = BTreeMap::new();
    let mut complete = input_complete;
    let mut failed_chunks = 0;
    let mut previous_end = 0;
    let mut tail = Vec::new();
    let mut last_report = Instant::now() - Duration::from_secs(1);
    for r in ranges {
        cancelled(cancel)?;
        if scanned + r.length as u64 > MAX_SCAN || started.elapsed() > MAX_TIME {
            complete = false;
            break;
        }
        let raw = match m.read(r.address, r.length) {
            Ok(b) => b,
            Err(e) if m.strict_errors() => return Err(e.into()),
            Err(_) => {
                failed_chunks += 1;
                tail.clear();
                previous_end = 0;
                continue;
            }
        };
        scanned += raw.len() as u64;
        if previous_end != r.address {
            tail.clear()
        }
        let keep = tail.len();
        tail.extend_from_slice(&raw);
        for (vt, family) in &targets {
            for offset in memchr::memmem::find_iter(&tail, &vt.to_le_bytes()) {
                let a = r.address - keep as u64 + offset as u64;
                if a.is_multiple_of(8) {
                    candidates.insert(a, (*vt, *family));
                    if candidates.len() > 100_000 {
                        return Err("Слишком много кандидатов; профиль не подтверждён".into());
                    }
                }
            }
        }
        let start = tail.len().saturating_sub(7);
        tail = tail[start..].to_vec();
        previous_end = r.address + r.length as u64;
        if last_report.elapsed() > Duration::from_millis(350) {
            progress(AnalysisProgress {
                stage: "Поиск объектов в доступной памяти".into(),
                scanned_bytes: scanned,
                total_bytes,
                object_count: candidates.len() as u64,
            });
            last_report = Instant::now();
        }
    }
    let mut scene=Scene{format:1,source:source.into(),started_at,captured_at:stamp(),complete,profile:profile.id().into(),objects:Vec::new(),meshes:Default::default(),camera_heading:None,players:Vec::new(),zones:Vec::new(),zones_fresh:false,warnings:vec!["Снимок не атомарен: объекты прочитаны в разные моменты времени.".into(),"Геометрия исследовательская: принадлежность текущему региону и переходы между частями не гарантированы.".into(),"По координатам нельзя установить, доступен ли предмет и был ли он подобран.".into()],stats:SceneStats{scanned_bytes:scanned,..Default::default()},identities:Vec::new(),process:None,discovery_profile:Some(profile.clone()),discovery_ranges:Vec::new(),discovery_cursor:0,filter_discovery:Default::default(),registry_discovery:Default::default()};
    if failed_chunks > 0 {
        scene.complete = false;
        scene.warnings.push(format!(
            "Не удалось прочитать изменившихся блоков: {failed_chunks}."
        ))
    }
    if !complete {
        scene.warnings.push("Обход неполный: исходный снимок содержит пропуски или достигнут предел времени/объёма.".into());
    }
    let mut rejected = 0;
    let mut mesh_rejected = 0;
    for (a, (vt, family)) in candidates {
        cancelled(cancel)?;
        if started.elapsed() > MAX_TIME || scene.objects.len() >= MAX_OBJECTS {
            scene.complete = false;
            scene
                .warnings
                .push("Достигнут предел обработки объектов.".into());
            break;
        }
        if family == "level" {
            if identity(m, a, vt).is_ok() {
                match geometry::mesh(m, a, &profile) {
                    Ok(mesh)
                        if scene.meshes.len() < 256
                            && scene.stats.vertex_count + mesh.vertices.len() as u64
                                <= 1_000_000 =>
                    {
                        scene.stats.vertex_count += mesh.vertices.len() as u64;
                        scene.stats.face_count += mesh.faces.len() as u64;
                        std::sync::Arc::make_mut(&mut scene.meshes).push(mesh)
                    }
                    _ => mesh_rejected += 1,
                }
            }
        } else {
            match object(m, a, family, vt, &mut decoder, &profile) {
                Ok((obj, id)) => {
                    scene.objects.push(obj);
                    scene.identities.push(id)
                }
                Err(_) => rejected += 1,
            }
        }
        if last_report.elapsed() > Duration::from_millis(350) {
            progress(AnalysisProgress {
                stage: "Проверка типов, связей и геометрии".into(),
                scanned_bytes: scanned,
                total_bytes,
                object_count: scene.objects.len() as u64,
            });
            last_report = Instant::now();
        }
    }
    if mesh_rejected > 0 {
        scene.warnings.push(format!(
            "Частей геометрии не прошло проверку: {mesh_rejected}."
        ));
    }
    if rejected > 0 {
        scene.warnings.push(format!(
            "Шаблонов или неподтверждённых объектов исключено: {rejected}."
        ));
    }
    let source_ranges = m.ranges();
    let mut pools = BTreeMap::new();
    for id in &scene.identities {
        let i = source_ranges.partition_point(|r| r.address <= id.address);
        if let Some(range) = i.checked_sub(1).and_then(|i| source_ranges.get(i))
            && id.address < range.address + range.length as u64
        {
            pools.insert(range.address, *range);
        }
    }
    scene.discovery_ranges = pools.into_values().collect();
    super::context::update(m, &mut scene, &profile, cancel)?;
    // При полном обходе в памяти могут оставаться действия прошлой миссии.
    let root = local_mission_root(m, &scene);
    let mut removed = HashSet::new();
    scene.objects.retain(|object| {
        let keep = !matches!(object.kind.as_str(), "dragon_door" | "decree_fragment")
            || root.is_some_and(|root| {
                u64::from_str_radix(object.key.trim_start_matches("0x"), 16)
                    .ok()
                    .and_then(|a| q(m, a + 0x1e0).ok())
                    == Some(root)
            });
        if !keep {
            removed.insert(object.key.clone());
        }
        keep
    });
    scene
        .identities
        .retain(|id| !removed.contains(&format!("0x{:x}", id.address)));
    scene.stats.object_count = scene.objects.len() as u64;
    scene.stats.mesh_count = scene.meshes.len() as u64;
    scene.captured_at = stamp();
    progress(AnalysisProgress {
        stage: "Готово".into(),
        scanned_bytes: scanned,
        total_bytes,
        object_count: scene.stats.object_count,
    });
    Ok(scene)
}
pub fn analyze_archive(
    path: &Path,
    sequence: u64,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> Result<Scene> {
    let pack = ProfilePack::bundled()?;
    analyze_archive_with_profiles(path, sequence, &pack, cancel, progress)
}

pub fn analyze_archive_with_profiles(
    path: &Path,
    sequence: u64,
    pack: &ProfilePack,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> Result<Scene> {
    analyze_archive_with_profiles_detailed(path, sequence, pack, cancel, progress)
        .map_err(|error| error.to_string())
}

pub fn analyze_archive_with_profiles_detailed(
    path: &Path,
    sequence: u64,
    pack: &ProfilePack,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> std::result::Result<Scene, AnalysisFailure> {
    let mut memory = ArchiveMemory::open(path, sequence, cancel)?;
    let started = memory.started_at.clone();
    let complete = memory.complete;
    let captured = memory.ended_at.clone();
    let mut scene = run(
        &mut memory,
        pack,
        "archive",
        started,
        complete,
        cancel,
        &progress,
    )?;
    scene.captured_at = captured;
    Ok(scene)
}
pub fn analyze_live(
    pid: u32,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> Result<Scene> {
    let pack = ProfilePack::bundled()?;
    analyze_live_with_profiles(pid, &pack, cancel, progress)
}

pub fn analyze_live_with_profiles(
    pid: u32,
    pack: &ProfilePack,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> Result<Scene> {
    analyze_live_with_profiles_detailed(pid, pack, cancel, progress)
        .map_err(|error| error.to_string())
}

pub fn analyze_live_with_profiles_detailed(
    pid: u32,
    pack: &ProfilePack,
    cancel: &AtomicBool,
    progress: impl Fn(AnalysisProgress),
) -> std::result::Result<Scene, AnalysisFailure> {
    let _lock = crate::squad::SCAN_LOCK
        .try_lock()
        .map_err(|_| "Другое чтение памяти ещё выполняется")?;
    let mut m = LiveMemory::open(pid, cancel)?;
    let created = m.process.created.clone();
    let mut scene = run(&mut m, pack, "live", stamp(), true, cancel, &progress)?;
    if !m.process.alive() {
        return Err("Игра завершилась во время исследования".into());
    }
    scene.process = Some((pid, created));
    Ok(scene)
}
/// Компактная привязка: без геометрии, пулов, списка объектов и глобального поиска.
#[derive(Clone)]
pub struct LocalPoseReader {
    pid: u32,
    created: String,
    profile: Profile,
    player: u64,
    identity: Identity,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPose {
    pub avatar_key: String,
    pub position: Option<[f32; 3]>,
    pub camera_heading: Option<f32>,
}
impl LocalPoseReader {
    pub fn from_scene(scene: &Scene) -> Option<Self> {
        if scene.source != "live" {
            return None;
        }
        let mut locals = scene.players.iter().filter(|p| p.local);
        let local = locals.next()?;
        if locals.next().is_some() {
            return None;
        }
        let avatar = u64::from_str_radix(local.avatar_key.trim_start_matches("0x"), 16).ok()?;
        let player = u64::from_str_radix(local.key.trim_start_matches("0x"), 16).ok()?;
        let identity = scene
            .identities
            .iter()
            .find(|i| i.address == avatar && i.moving)?
            .clone();
        let (pid, created) = scene.process.as_ref()?;
        Some(Self {
            pid: *pid,
            created: created.clone(),
            profile: scene.discovery_profile.clone()?,
            player,
            identity,
        })
    }
    fn sample(&self, m: &mut dyn Memory) -> Result<LocalPose> {
        let avatar = self.identity.address;
        super::context::local_map(m, self.player, avatar, &self.profile)?;
        let Some((_, position)) = observe(m, &self.identity)? else {
            return Err("Объект персонажа изменился".into());
        };
        let camera_heading =
            super::context::camera_heading(m, self.player, avatar, &self.profile).ok();
        super::context::local_map(m, self.player, avatar, &self.profile)?;
        Ok(LocalPose {
            avatar_key: format!("0x{avatar:x}"),
            position,
            camera_heading,
        })
    }
    pub fn read(&self) -> Result<LocalPose> {
        let process = crate::binary_snapshot::Process::open(self.pid).map_err(|e| e.to_string())?;
        if process.created != self.created {
            return Err("Процесс игры изменился".into());
        }
        let mut m = LiveMemory {
            process,
            ranges: Vec::new(),
            modules: Vec::new(),
        };
        let pose = self.sample(&mut m)?;
        if !m.process.alive() {
            return Err("Игра завершилась".into());
        }
        Ok(pose)
    }
}

pub fn refresh_live(pid: u32, scene: &Scene, cancel: &AtomicBool) -> Result<Scene> {
    refresh_live_filtered(pid, scene, &DiscoveryRules::default(), cancel)
}
pub fn refresh_live_filtered(
    pid: u32,
    scene: &Scene,
    rules: &DiscoveryRules,
    cancel: &AtomicBool,
) -> Result<Scene> {
    if scene.source != "live" {
        return Err("Обновление доступно только для живой сцены".into());
    }
    let Some((original_pid, created)) = &scene.process else {
        return Err("Сцена загружена без идентичности процесса; выполните новый поиск".into());
    };
    if *original_pid != pid {
        return Err("Процесс сцены изменился".into());
    }
    let process = crate::binary_snapshot::Process::open(pid).map_err(|e| e.to_string())?;
    if process.created != *created {
        return Err("Игра перезапущена; нужен новый поиск".into());
    }
    let mut m = LiveMemory {
        process,
        ranges: Vec::new(),
        modules: Vec::new(),
    };
    let mut result = refresh_objects(&mut m, scene, cancel)?;
    if let Some(profile) = &scene.discovery_profile {
        super::context::update(&mut m, &mut result, profile, cancel)?;
    }
    let root = local_mission_root(&mut m, &result);
    let mut removed = HashSet::new();
    result.objects.retain(|object| {
        let keep = object.kind != "decree_fragment"
            || root.is_some_and(|root| {
                u64::from_str_radix(object.key.trim_start_matches("0x"), 16)
                    .ok()
                    .and_then(|a| q(&mut m, a + 0x1e0).ok())
                    == Some(root)
            });
        if !keep {
            removed.insert(object.key.clone());
        }
        keep
    });
    result
        .identities
        .retain(|id| !removed.contains(&format!("0x{:x}", id.address)));
    if !super::registry_discovery::discover(&mut m, &mut result, rules, cancel)? {
        super::filter_discovery::discover(&mut m, &mut result, rules, cancel)?;
        discover(&mut m, &mut result, cancel)?;
    }
    result.stats.object_count = result.objects.len() as u64;
    result.captured_at = stamp();
    if !m.process.alive() {
        return Err("Игра завершилась".into());
    }
    Ok(result)
}

// Читаем уже найденный объект по его идентичности. Переход игрока в другую Zone
// и рассогласованный кадр анимации не означают уничтожение персонажа.
fn observe(m: &mut dyn Memory, old: &Identity) -> Result<Option<(Identity, Option<[f32; 3]>)>> {
    let header = m.read(old.address, 24)?;
    if u64_at(&header, 0)? != old.vtable
        || u64_at(&header, 8)? != old.metadata
        || u64_at(&header, 16)? != old.handle
        || q(m, old.handle)? != old.address
    {
        return Ok(None);
    }
    let mut now = old.clone();
    now.missed_reads = 0;
    if old.moving {
        // У одного персонажа меняется Zone при переходах между комнатами.
        if let Ok((_, zone)) = handle(m, old.address + 0x428) {
            now.zone = Some(zone);
        }
    } else if let Some(zone) = old.zone
        && handle(m, old.address + 0x428)?.1 != zone
    {
        return Ok(None);
    }
    if let Some(item) = old.item
        && handle(m, old.address + old.item_offset)?.1 != item
    {
        return Ok(None);
    }
    if let Some(action) = old.action
        && handle(m, old.address + 0x488)?.1 != action
    {
        return Ok(None);
    }
    let position = if old.decree_fragment {
        let action = old.action.ok_or("Нет действия фрагмента")?;
        Some(decree_fragment_anchor(m, old.address, action)?)
    } else {
        geometry::position(m, old.address, old.moving).ok()
    };
    Ok(Some((now, position)))
}
fn refresh_objects(m: &mut dyn Memory, scene: &Scene, cancel: &AtomicBool) -> Result<Scene> {
    let mut result = scene.clone();
    let mut observed = std::collections::HashMap::new();
    let mut identities = Vec::new();
    for old in &scene.identities {
        cancelled(cancel)?;
        let observation = match observe(m, old) {
            Ok(value) => value,
            // Короткий сбой чтения не сбрасывает выбор пользователя. После трёх
            // подряд непрочитанных проверок объект убирается; замена — сразу.
            Err(_) if old.missed_reads < 2 => {
                let mut retained = old.clone();
                retained.missed_reads += 1;
                Some((retained, None))
            }
            Err(_) => None,
        };
        if let Some((id, position)) = observation {
            observed.insert(format!("0x{:x}", id.address), position);
            identities.push(id);
        }
    }
    result.objects.retain_mut(|object| {
        let Some(position) = observed.get(&object.key) else {
            return false;
        };
        object.position_fresh = position.is_some();
        if let Some(position) = position {
            object.position = *position;
        }
        true
    });
    result.identities = identities;
    if let Some(profile) = &scene.discovery_profile {
        let mut decoder = Decoder::new(profile.clone())?;
        for object in result
            .objects
            .iter_mut()
            .filter(|o| o.kind == "cache")
            .take(64)
        {
            let id = result
                .identities
                .iter()
                .find(|id| format!("0x{:x}", id.address) == object.key);
            object.availability = if let Some(id) = id.filter(|id| id.missed_reads == 0) {
                super::cache::state(m, id.address, &mut decoder, profile).unwrap_or("unknown")
            } else {
                "unknown"
            }
            .into();
        }
    }
    result.stats.object_count = result.objects.len() as u64;
    Ok(result)
}

/// Ограниченный поиск соседей в уже подтверждённых пулах. Новые области добавляются только ручным чтением.
fn discover(m: &mut dyn Memory, scene: &mut Scene, cancel: &AtomicBool) -> Result<()> {
    let Some(profile) = scene.discovery_profile.clone() else {
        return Ok(());
    };
    if scene.discovery_ranges.is_empty() || scene.objects.len() >= MAX_OBJECTS {
        return Ok(());
    }
    let mut decoder = Decoder::new(profile.clone())?;
    let targets = profile.targets()?;
    let known: HashSet<u64> = scene.identities.iter().map(|i| i.address).collect();
    let started = Instant::now();
    let mut found = BTreeMap::new();
    for _ in 0..2 {
        cancelled(cancel)?;
        let r = scene.discovery_ranges[scene.discovery_cursor % scene.discovery_ranges.len()];
        scene.discovery_cursor = (scene.discovery_cursor + 1) % scene.discovery_ranges.len();
        let Ok(raw) = m.read(r.address, r.length) else {
            continue;
        };
        scene.stats.scanned_bytes = scene.stats.scanned_bytes.saturating_add(raw.len() as u64);
        for (vt, family) in &targets {
            if *family == "level" {
                continue;
            }
            for offset in memchr::memmem::find_iter(&raw, &vt.to_le_bytes()) {
                let a = r.address + offset as u64;
                if a.is_multiple_of(8) && !known.contains(&a) {
                    found.insert(a, (*vt, *family));
                }
            }
        }
    }
    for (a, (vt, family)) in found {
        cancelled(cancel)?;
        if started.elapsed() > Duration::from_millis(80) || scene.objects.len() >= MAX_OBJECTS {
            break;
        }
        if family == "decree_fragment"
            && !local_mission_root(m, scene).is_some_and(|root| q(m, a + 0x1e0).ok() == Some(root))
        {
            continue;
        }
        if let Ok((object, id)) = object(m, a, family, vt, &mut decoder, &profile) {
            if family == "decree_fragment"
                && !local_mission_root(m, scene)
                    .is_some_and(|root| q(m, a + 0x1e0).ok() == Some(root))
            {
                continue;
            }
            scene.objects.push(object);
            scene.identities.push(id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct BrokenProfileMemory;
    impl Memory for BrokenProfileMemory {
        fn read(&mut self, _: u64, _: usize) -> Result<Vec<u8>> {
            Err("Нет страницы по старому RVA".into())
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            vec![]
        }
        fn modules(&self) -> Vec<MemoryModule> {
            vec![MemoryModule {
                name: "Warframe.x64.exe".into(),
                base: 0x1000,
                size: 47_116_288,
            }]
        }
    }

    #[test]
    fn unreadable_old_profile_triggers_profile_update() {
        let pack = ProfilePack::bundled().unwrap();
        let cancel = AtomicBool::new(false);
        let result = run(
            &mut BrokenProfileMemory,
            &pack,
            "archive",
            "test".into(),
            true,
            &cancel,
            &|_| {},
        );
        assert!(matches!(
            result,
            Err(AnalysisFailure::UnsupportedBuild(message)) if message == "Нет страницы по старому RVA"
        ));
    }

    struct Mock {
        bytes: Vec<u8>,
        fail: bool,
    }
    impl Memory for Mock {
        fn read(&mut self, address: u64, length: usize) -> Result<Vec<u8>> {
            if self.fail {
                return Err("Временный сбой".into());
            }
            self.bytes
                .get(address as usize..address as usize + length)
                .map(<[u8]>::to_vec)
                .ok_or("Нет области".into())
        }
        fn ranges(&self) -> Vec<MemoryRange> {
            vec![]
        }
        fn modules(&self) -> Vec<MemoryModule> {
            vec![]
        }
    }
    fn put_q(m: &mut Mock, address: usize, value: u64) {
        m.bytes[address..address + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn put_f(m: &mut Mock, address: usize, value: f32) {
        m.bytes[address..address + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> (Mock, Scene) {
        let mut m = Mock {
            bytes: vec![0; 0x4000],
            fail: false,
        };
        for (a, v) in [
            (0x100, 0x10),
            (0x108, 0x20),
            (0x110, 0x800),
            (0x800, 0x100),
            (0x528, 0x900),
            (0x900, 0x1000),
            (0x1010, 0x900),
        ] {
            put_q(&mut m, a, v);
        }
        for i in 0..4 {
            put_f(&mut m, 0x1a0 + i * 20, 1.0);
        }
        for (i, v) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            put_f(&mut m, 0x170 + i * 4, v);
            put_f(&mut m, 0x1d0 + i * 4, v);
        }
        let mut scene: Scene = serde_json::from_value(serde_json::json!({
            "format":1,"source":"live","startedAt":"test","capturedAt":"test","complete":true,"profile":"test",
            "objects":[{"key":"0x100","kind":"avatar","label":"Оператор","nameEn":"Operator","itemPath":null,"position":[1,2,3],"typeNames":["TennoAvatar *"],"availability":"unknown","details":[]}],
            "meshes":[],"warnings":[],"stats":{"scannedBytes":0,"objectCount":1,"meshCount":0,"vertexCount":0,"faceCount":0}
        })).unwrap();
        scene.identities.push(Identity {
            address: 0x100,
            vtable: 0x10,
            metadata: 0x20,
            handle: 0x800,
            zone: Some(0x1000),
            item: None,
            action: None,
            item_offset: 0x528,
            moving: true,
            decree_fragment: false,
            missed_reads: 0,
        });
        (m, scene)
    }
    #[test]
    fn fast_pose_reads_only_local_links_and_rejects_replaced_avatar() {
        let (mut m, scene) = fixture();
        m.bytes.resize(0x3000000, 0);
        for (object, meta) in [
            (0x3000, 0x29c9600),
            (0x6000, 0x28cc310),
            (0x8000, 0x29e7ea0),
        ] {
            put_q(&mut m, object + 8, meta as u64);
            put_q(&mut m, object + 16, (object + 0x800) as u64);
            put_q(&mut m, object + 0x800, object as u64);
            put_q(&mut m, meta, 0x203fc60);
        }
        for (at, value) in [
            (0x620, 0x3800),
            (0x3118, 0x800),
            (0x3030, 0x6800),
            (0x10098, 0x8800),
            (0x80e0, 0x800),
        ] {
            put_q(&mut m, at, value);
        }
        for i in 0..4 {
            put_f(&mut m, 0x60a0 + i * 20, 1.0);
        }
        let reader = LocalPoseReader {
            pid: 1,
            created: String::new(),
            profile: Profile::legacy(0),
            player: 0x3000,
            identity: scene.identities[0].clone(),
        };
        let pose = reader.sample(&mut m).unwrap();
        assert_eq!(pose.position, Some([1., 2., 3.]));
        assert_eq!(pose.camera_heading, Some(0.));
        // В памяти нет массивов зон или геометрии — адресного чтения достаточно.
        put_f(&mut m, 0x170, 7.);
        put_f(&mut m, 0x1d0, 7.);
        assert_eq!(reader.sample(&mut m).unwrap().position, Some([7., 2., 3.]));
        put_q(&mut m, 0x108, 0x21);
        assert!(reader.sample(&mut m).is_err());
    }

    #[test]
    fn npc_and_hostage_are_not_player_avatars() {
        assert_eq!(character_kind("npc", Some("CorpusCrewman"), None), "npc");
        assert_eq!(
            character_kind("npc", None, Some("/Lotus/Language/Game/Operator")),
            "npc"
        );
        assert_eq!(character_kind("npc", Some("Hostage"), None), "hostage");
        assert_eq!(character_kind("avatar", None, None), "avatar");
    }
    #[test]
    fn player_keeps_identity_and_coordinates_after_entering_another_zone() {
        let (mut m, scene) = fixture();
        put_q(&mut m, 0x900, 0x2000);
        put_q(&mut m, 0x2010, 0x900);
        put_f(&mut m, 0x170, 5.0);
        put_f(&mut m, 0x1d0, 5.0);
        let next = refresh_objects(&mut m, &scene, &AtomicBool::new(false)).unwrap();
        assert_eq!(next.objects[0].key, "0x100");
        assert_eq!(next.objects[0].position, [5.0, 2.0, 3.0]);
        assert!(next.objects[0].position_fresh);
        assert_eq!(next.identities[0].zone, Some(0x2000));
        let mut stationary = scene;
        stationary.identities[0].moving = false;
        assert!(
            refresh_objects(&mut m, &stationary, &AtomicBool::new(false))
                .unwrap()
                .objects
                .is_empty()
        );
    }
    #[test]
    fn animation_read_mismatch_keeps_selection_then_recovers_position() {
        let (mut m, scene) = fixture();
        put_f(&mut m, 0x170, 50.0);
        let stale = refresh_objects(&mut m, &scene, &AtomicBool::new(false)).unwrap();
        assert_eq!(stale.objects[0].position, [1.0, 2.0, 3.0]);
        assert!(!stale.objects[0].position_fresh);
        put_f(&mut m, 0x1d0, 50.0);
        let fresh = refresh_objects(&mut m, &stale, &AtomicBool::new(false)).unwrap();
        assert!(fresh.objects[0].position_fresh);
        assert_eq!(fresh.objects[0].position, [50.0, 2.0, 3.0]);
        put_q(&mut m, 0x108, 0x21);
        assert!(
            refresh_objects(&mut m, &fresh, &AtomicBool::new(false))
                .unwrap()
                .objects
                .is_empty()
        );
    }
    #[test]
    fn short_read_failure_does_not_remove_player_but_repeated_failures_do() {
        let (mut m, scene) = fixture();
        m.fail = true;
        let once = refresh_objects(&mut m, &scene, &AtomicBool::new(false)).unwrap();
        let twice = refresh_objects(&mut m, &once, &AtomicBool::new(false)).unwrap();
        assert_eq!(twice.objects.len(), 1);
        assert!(!twice.objects[0].position_fresh);
        m.fail = false;
        assert!(
            refresh_objects(&mut m, &twice, &AtomicBool::new(false))
                .unwrap()
                .objects[0]
                .position_fresh
        );
        m.fail = true;
        assert!(
            refresh_objects(&mut m, &twice, &AtomicBool::new(false))
                .unwrap()
                .objects
                .is_empty()
        );
    }
    #[test]
    #[ignore = "Ручная проверка на локальном архиве: PLATSCOPE_DECREE_ARCHIVE"]
    fn archived_duviri_decree_fragments_have_confirmed_links() {
        let archive = std::env::var("PLATSCOPE_DECREE_ARCHIVE").unwrap();
        let mut memory =
            ArchiveMemory::open(Path::new(&archive), 2, &AtomicBool::new(false)).unwrap();
        let profile = Profile::validate(&mut memory).unwrap();
        let vt = profile
            .targets()
            .unwrap()
            .into_iter()
            .find(|(_, family)| *family == "decree_fragment")
            .unwrap()
            .0;
        let mut decoder = Decoder::new(profile.clone()).unwrap();
        for address in [0x1d73c0f0500, 0x1d783f4be80] {
            let (fragment, id) = object(
                &mut memory,
                address,
                "decree_fragment",
                vt,
                &mut decoder,
                &profile,
            )
            .unwrap();
            assert_eq!(fragment.kind, "decree_fragment");
            assert_eq!(fragment.item_path.as_deref(), Some("DuviriArenaBoonItem"));
            assert!(id.zone.is_some() && id.item.is_some());
            assert_eq!(
                observe(&mut memory, &id).unwrap().unwrap().1,
                Some(fragment.position)
            );
        }
    }
}
