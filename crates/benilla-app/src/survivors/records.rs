//! The arcade records: every zone keeps a top ten, as an old cabinet does (`docs/PROGRESSION.md`).
//!
//! A run that makes its zone's table, won or lost, enters it under the character's full name
//! straight away, and the results screen says "New record!" with its place.
//! A **Records** button under the character list opens the tables, one zone at a time, the
//! playing character's entries lit. Runs the auto-player drives (`super::auto`) never enter.
//!
//! Saved in `benilla-config/records.toml`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::auto::Auto;
use super::characters::{Characters, LeftSlot};
use super::data::{self, Difficulty};
use super::hud::{rgb, HudFont};
use super::{Phase, Run, Selection};

/// Entries a zone's table keeps.
pub(super) const TABLE: usize = 10;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const DIM: Color = Color::srgb(0.6, 0.55, 0.45);
const FILL: Color = Color::srgba(0.03, 0.025, 0.02, 0.97);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const BUTTON_HOT: Color = Color::srgba(0.34, 0.22, 0.06, 0.95);
const MINE: Color = Color::srgba(0.45, 0.3, 0.05, 0.6);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct Record {
    /// The character's full name. Files written before names were kept call it `initials`.
    #[serde(alias = "initials")]
    pub name: String,
    pub kills: u32,
    /// Seconds survived.
    pub secs: f32,
    pub class: String,
    pub difficulty: String,
    pub won: bool,
    pub level: u32,
    /// Unix seconds.
    pub date: u64,
    /// The character that set it, to light its own entries.
    #[serde(default)]
    pub character: Option<u32>,
}

impl Record {
    /// Better first: more kills, then longer alive.
    fn beats(&self, other: &Record) -> bool {
        (self.kills, self.secs) > (other.kills, other.secs)
    }
}

/// Every zone's table, by zone name.
#[derive(Resource, Serialize, Deserialize, Default)]
pub(super) struct Records {
    #[serde(default)]
    pub zones: std::collections::BTreeMap<String, Vec<Record>>,
}

impl Records {
    fn table(&self, map: &str) -> &[Record] {
        self.zones.get(map).map_or(&[], Vec::as_slice)
    }

    /// Whether `r` would enter `map`'s table.
    fn qualifies(&self, map: &str, r: &Record) -> bool {
        let t = self.table(map);
        t.len() < TABLE || t.last().is_some_and(|last| r.beats(last))
    }

    /// Insert `r`, keep the ten best; returns its place, 0 first.
    fn insert(&mut self, map: &str, r: Record) -> usize {
        let t = self.zones.entry(map.to_string()).or_default();
        let at = t.iter().position(|e| r.beats(e)).unwrap_or(t.len());
        t.insert(at, r);
        t.truncate(TABLE);
        at
    }
}

fn path() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("records.toml"))
}

fn save(records: &Records) {
    if let (Some(p), Ok(text)) = (path(), toml::to_string(records)) {
        let _ = crate::local_state::write_atomic(&p, &text);
    }
}

fn load() -> Records {
    path()
        .and_then(|p| crate::local_state::read_to_string(&p).ok())
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

/// The records page: open, and the zone shown (an index into the menu's zone order).
#[derive(Resource, Default)]
struct Page {
    open: bool,
    zone: usize,
    version: u32,
}

/// The results screen's "New record!" line, placed by `hud::setup_hud`; empty when the run
/// made no table.
#[derive(Component)]
pub(super) struct RecordLine;
#[derive(Component)]
struct PageRoot;
#[derive(Component)]
struct PageTitle;
#[derive(Component)]
struct PageRows;
#[derive(Component)]
struct OpenButton;

#[derive(Component, Clone, Copy, PartialEq)]
enum RecButton {
    Open,
    Prev,
    Next,
    Close,
}

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(load())
        .init_resource::<Page>()
        .add_systems(Startup, spawn_ui.after(super::characters::spawn_ui))
        .add_systems(OnEnter(Phase::GameOver), store_record)
        .add_systems(Update, (press, show, rebuild_page).chain());
}

fn text(font: &Handle<Font>, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
        TextShadow {
            offset: Vec2::splat(1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.85),
        },
        Pickable::IGNORE,
    )
}

fn button(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    action: RecButton,
    label: &str,
    width: f32,
    height: f32,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                width: Val::Px(width),
                height: Val::Px(height),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(BUTTON),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            b.spawn(text(font, label, 18.0, Color::WHITE));
        });
}

fn spawn_ui(mut commands: Commands, font: Option<Res<HudFont>>, slots: Query<(Entity, &LeftSlot)>) {
    let Some(font) = font else { return };
    let f = &font.0;
    // The records page, over the menu.
    commands
        .spawn((
            PageRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.92)),
            // Over the menu screen (410), so its zone list and buttons never show through.
            GlobalZIndex(455),
        ))
        .with_children(|root| {
            root.spawn(text(f, "RECORDS", 44.0, GOLD));
            root.spawn(Node {
                column_gap: Val::Px(14.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                button(row, f, RecButton::Prev, "<", 52.0, 44.0);
                row.spawn((
                    Node {
                        width: Val::Px(360.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|c| {
                    c.spawn((text(f, "", 24.0, Color::WHITE), PageTitle));
                });
                button(row, f, RecButton::Next, ">", 52.0, 44.0);
            });
            root.spawn((
                PageRows,
                Node {
                    // Wide enough for a twelve-letter name column (`NAME_MAX`).
                    width: Val::Px(800.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(FILL),
                BorderColor::all(EDGE),
            ));
            button(root, f, RecButton::Close, "Back", 240.0, 44.0);
        });
    // Its button, at the foot of the character screen's left column.
    let Some(slot) = slots
        .iter()
        .find_map(|(e, s)| (*s == LeftSlot::Records).then_some(e))
    else {
        return;
    };
    commands.entity(slot).with_children(|c| {
        c.spawn((
            OpenButton,
            Node {
                width: Val::Px(236.0),
                margin: UiRect::top(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|c| button(c, f, RecButton::Open, "Records", 236.0, 44.0));
    });
}

/// The name a record keeps: the character's full name (at most [`NAME_MAX`] characters), "Demo"
/// for a run the auto-player drove, "Hero" when there is no character.
pub(super) fn record_name(character: Option<&str>, demo: bool) -> String {
    if demo {
        return "Demo".to_string();
    }
    match character.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.chars().take(NAME_MAX).collect(),
        None => "Hero".to_string(),
    }
}

/// The longest name the table's name column fits.
pub(super) const NAME_MAX: usize = 12;

/// At the results: a run that makes its zone's table goes in under the character's name, and
/// the results screen's line says so.
fn store_record(
    run: Res<Run>,
    selection: Res<Selection>,
    mut records: ResMut<Records>,
    chars: Res<Characters>,
    auto: Option<Res<Auto>>,
    mut page: ResMut<Page>,
    mut lines: Query<&mut Text, With<RecordLine>>,
) {
    let mut line = String::new();
    if super::secrets::records_progress(auto.is_some(), super::hero::god_mode()) && run.kills > 0 {
        let map = data::MAPS[selection.stage.min(data::MAPS.len() - 1)].name;
        let unix = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let character = selection
            .character
            .and_then(|id| chars.list.iter().find(|c| c.id == id))
            .map(|c| c.name.as_str());
        let record = Record {
            name: record_name(character, auto.is_some()),
            kills: run.kills,
            secs: run.elapsed,
            class: run.class.name.to_string(),
            difficulty: data::difficulty().name().to_string(),
            won: run.won,
            level: run.level,
            date: unix,
            character: selection.character,
        };
        if records.qualifies(map, &record) {
            let place = records.insert(map, record);
            save(&records);
            super::combatlog::event(0.0, format!("record: {map}, place {}", place + 1));
            page.version += 1;
            line = format!("New record! #{} in {map}", place + 1);
        }
    }
    for mut t in &mut lines {
        if t.0 != line {
            t.0 = line.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn press(
    mut buttons: Query<(&Interaction, &RecButton, &mut BackgroundColor), Changed<Interaction>>,
    mut page: ResMut<Page>,
    selection: Res<Selection>,
) {
    for (interaction, action, mut bg) in &mut buttons {
        bg.0 = match interaction {
            Interaction::None => BUTTON,
            _ => BUTTON_HOT,
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        let zones = super::menu::zone_order();
        match *action {
            RecButton::Open => {
                page.open = true;
                page.zone = zones
                    .iter()
                    .position(|(m, ..)| *m == selection.map)
                    .unwrap_or(0);
                page.version += 1;
            }
            RecButton::Prev => {
                page.zone = (page.zone + zones.len() - 1) % zones.len();
                page.version += 1;
            }
            RecButton::Next => {
                page.zone = (page.zone + 1) % zones.len();
                page.version += 1;
            }
            RecButton::Close => page.open = false,
        }
    }
}

#[allow(clippy::type_complexity)]
fn show(
    phase: Res<State<Phase>>,
    mut page: ResMut<Page>,
    selection: Res<crate::survivors::Selection>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<PageRoot>>,
        Query<&mut Node, With<OpenButton>>,
    )>,
) {
    let menu = *phase.get() == Phase::Menu;
    if !menu && page.open {
        page.open = false;
    }
    let set = |mut n: Mut<Node>, on: bool| {
        let d = if on { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    };
    for n in &mut nodes.p0() {
        set(n, menu && page.open);
    }
    // Create Character has no opener, as for Talents and Secrets.
    let listing = selection.character.is_some();
    for n in &mut nodes.p1() {
        set(n, menu && listing && !page.open);
    }
}

fn rebuild_page(
    mut commands: Commands,
    page: Res<Page>,
    records: Res<Records>,
    selection: Res<Selection>,
    font: Option<Res<HudFont>>,
    rows: Query<Entity, With<PageRows>>,
    mut title: Query<&mut Text, With<PageTitle>>,
    mut built: Local<Option<u32>>,
) {
    if !page.open || *built == Some(page.version) {
        return;
    }
    let (Some(font), Ok(list)) = (font, rows.single()) else {
        return;
    };
    *built = Some(page.version);
    let f = &font.0;
    let zones = super::menu::zone_order();
    let Some(&(m, ..)) = zones.get(page.zone) else {
        return;
    };
    let map = data::MAPS[m].name;
    if let Ok(mut t) = title.single_mut() {
        t.0 = map.to_string();
    }
    let table = records.table(map).to_vec();
    commands.entity(list).despawn_related::<Children>();
    commands.entity(list).with_children(|ui| {
        if table.is_empty() {
            ui.spawn(text(f, "No records yet.", 18.0, DIM));
            return;
        }
        for (i, r) in table.iter().enumerate() {
            let mine = r.character.is_some() && r.character == selection.character;
            let class_color = data::CLASSES
                .iter()
                .find(|c| c.name == r.class)
                .map_or(Color::WHITE, |c| rgb(c.color));
            let secs = r.secs as u32;
            let difficulty_icon = Difficulty::ALL
                .into_iter()
                .find(|d| d.name() == r.difficulty)
                .unwrap_or_default()
                .icon();
            ui.spawn((
                Node {
                    height: Val::Px(30.0),
                    column_gap: Val::Px(14.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(if mine { MINE } else { Color::NONE }),
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                let cell = |w: f32| Node {
                    width: Val::Px(w),
                    ..default()
                };
                row.spawn((cell(36.0), Pickable::IGNORE)).with_child(text(
                    f,
                    format!("{}.", i + 1),
                    18.0,
                    DIM,
                ));
                // Twelve characters at 20 units, on one line.
                row.spawn((cell(150.0), Pickable::IGNORE)).with_child((
                    text(f, r.name.clone(), 20.0, GOLD),
                    TextLayout::new_with_no_wrap(),
                ));
                row.spawn((cell(110.0), Pickable::IGNORE)).with_child(text(
                    f,
                    format!("{} kills", r.kills),
                    18.0,
                    Color::WHITE,
                ));
                row.spawn((cell(70.0), Pickable::IGNORE)).with_child(text(
                    f,
                    format!("{:02}:{:02}", secs / 60, secs % 60),
                    18.0,
                    Color::WHITE,
                ));
                row.spawn((cell(110.0), Pickable::IGNORE)).with_child(text(
                    f,
                    r.class.clone(),
                    18.0,
                    class_color,
                ));
                row.spawn((
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    ImageNode {
                        color: Color::NONE,
                        ..default()
                    },
                    super::hud::UiArt::crop(difficulty_icon, [0.07, 0.93, 0.07, 0.93]),
                    Pickable::IGNORE,
                ));
                row.spawn((cell(90.0), Pickable::IGNORE)).with_child(text(
                    f,
                    if r.won { "Victory" } else { "" },
                    16.0,
                    GOLD,
                ));
                row.spawn((cell(80.0), Pickable::IGNORE)).with_child(text(
                    f,
                    format!("Lv {}", r.level),
                    16.0,
                    DIM,
                ));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_records_load_with_initials_as_name() {
        let old = r#"
[[zones."Elwynn Forest"]]
initials = "ABC"
kills = 120
secs = 300.0
class = "Warrior"
difficulty = "Normal"
won = false
level = 9
date = 1700000000
"#;
        let records: Records = toml::from_str(old).expect("old records load");
        let t = records.table("Elwynn Forest");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "ABC");
        assert_eq!(t[0].character, None);
        // Written back, the field is `name`.
        let text = toml::to_string(&records).unwrap();
        assert!(text.contains("name = \"ABC\""), "{text}");
        assert!(!text.contains("initials"), "{text}");
    }

    #[test]
    fn new_record_takes_character_name() {
        assert_eq!(record_name(Some("Thrallsworth"), false), "Thrallsworth");
        assert_eq!(record_name(Some("Bob"), false), "Bob");
        // Longer than the column: cut at twelve.
        assert_eq!(record_name(Some("Abcdefghijklmnop"), false), "Abcdefghijkl");
        assert_eq!(record_name(Some("Bob"), true), "Demo");
        assert_eq!(record_name(None, true), "Demo");
        assert_eq!(record_name(None, false), "Hero");
        assert_eq!(record_name(Some("  "), false), "Hero");
    }
}
