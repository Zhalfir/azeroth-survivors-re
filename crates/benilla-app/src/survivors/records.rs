//! The arcade records: every zone keeps a top ten, as an old cabinet does (`docs/PROGRESSION.md`).
//!
//! A run that makes its zone's table, won or lost, asks for three initials on the results screen,
//! cabinet style: + and - by each letter, OK to keep them. They start from the character's name.
//! A **Records** button under the character list opens the tables, one zone at a time, the
//! playing character's entries lit. Runs the auto-player drives (`super::auto`) never enter.
//!
//! Saved in `benilla-config/records.toml`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::auto::Auto;
use super::characters::Characters;
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
    pub initials: String,
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

/// The run waiting for its initials, and the three letters as they stand.
#[derive(Resource, Default)]
struct Entry {
    pending: Option<(String, Record)>,
    letters: [u8; 3],
}

/// The records page: open, and the zone shown (an index into the menu's zone order).
#[derive(Resource, Default)]
struct Page {
    open: bool,
    zone: usize,
    version: u32,
}

#[derive(Component)]
struct EntryRoot;
#[derive(Component)]
struct EntryLetter(usize);
#[derive(Component)]
struct EntryHeadline;
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
    Up(usize),
    Down(usize),
    Ok,
    Open,
    Prev,
    Next,
    Close,
}

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(load())
        .init_resource::<Entry>()
        .init_resource::<Page>()
        .add_systems(Startup, spawn_ui.after(super::hud::setup_hud))
        .add_systems(OnEnter(Phase::GameOver), offer_entry)
        .add_systems(OnExit(Phase::GameOver), drop_entry)
        .add_systems(Update, (press, show, refresh_entry, rebuild_page).chain());
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

fn spawn_ui(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
    // The initials prompt, over the results.
    commands
        .spawn((
            EntryRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.88)),
            GlobalZIndex(460),
        ))
        .with_children(|root| {
            root.spawn(text(f, "NEW RECORD!", 52.0, GOLD));
            root.spawn((text(f, "", 20.0, Color::WHITE), EntryHeadline));
            root.spawn(text(f, "Enter your initials", 18.0, DIM));
            root.spawn(Node {
                column_gap: Val::Px(26.0),
                ..default()
            })
            .with_children(|row| {
                for i in 0..3 {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|col| {
                        button(col, f, RecButton::Up(i), "+", 72.0, 52.0);
                        col.spawn((
                            Node {
                                width: Val::Px(72.0),
                                height: Val::Px(80.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(2.0)),
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(FILL),
                            BorderColor::all(GOLD),
                        ))
                        .with_children(|c| {
                            c.spawn((text(f, "A", 56.0, GOLD), EntryLetter(i)));
                        });
                        button(col, f, RecButton::Down(i), "-", 72.0, 52.0);
                    });
                }
            });
            button(root, f, RecButton::Ok, "OK", 240.0, 48.0);
        });
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
            GlobalZIndex(120),
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
                    width: Val::Px(700.0),
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
    // Its button, under the character list.
    commands
        .spawn((
            OpenButton,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(28.0),
                bottom: Val::Px(36.0),
                width: Val::Px(236.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|c| button(c, f, RecButton::Open, "Records", 236.0, 44.0));
}

/// The three letters a name starts with, `A` for the rest.
fn initials_of(name: &str) -> [u8; 3] {
    let mut out = [b'A'; 3];
    for (i, c) in name
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(3)
        .enumerate()
    {
        out[i] = c.to_ascii_uppercase() as u8;
    }
    out
}

fn offer_entry(
    run: Res<Run>,
    selection: Res<Selection>,
    records: Res<Records>,
    chars: Res<Characters>,
    auto: Option<Res<Auto>>,
    mut entry: ResMut<Entry>,
) {
    entry.pending = None;
    if auto.is_some() || run.kills == 0 {
        return;
    }
    let map = data::MAPS[selection.stage.min(data::MAPS.len() - 1)].name;
    let unix = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let record = Record {
        initials: String::new(),
        kills: run.kills,
        secs: run.elapsed,
        class: run.class.name.to_string(),
        difficulty: data::difficulty().name().to_string(),
        won: run.won,
        level: run.level,
        date: unix,
        character: selection.character,
    };
    if !records.qualifies(map, &record) {
        return;
    }
    let name = selection
        .character
        .and_then(|id| chars.list.iter().find(|c| c.id == id))
        .map_or("AAA", |c| c.name.as_str());
    entry.letters = initials_of(name);
    entry.pending = Some((map.to_string(), record));
}

fn drop_entry(mut entry: ResMut<Entry>) {
    entry.pending = None;
}

#[allow(clippy::too_many_arguments)]
fn press(
    mut buttons: Query<(&Interaction, &RecButton, &mut BackgroundColor), Changed<Interaction>>,
    mut entry: ResMut<Entry>,
    mut records: ResMut<Records>,
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
            RecButton::Up(i) => {
                let c = &mut entry.letters[i];
                *c = if *c >= b'Z' { b'A' } else { *c + 1 };
            }
            RecButton::Down(i) => {
                let c = &mut entry.letters[i];
                *c = if *c <= b'A' { b'Z' } else { *c - 1 };
            }
            RecButton::Ok => {
                let letters = entry.letters;
                if let Some((map, mut record)) = entry.pending.take() {
                    record.initials = String::from_utf8_lossy(&letters).into_owned();
                    let place = records.insert(&map, record);
                    save(&records);
                    super::combatlog::event(0.0, format!("record: {map}, place {}", place + 1));
                    page.version += 1;
                }
            }
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
    entry: Res<Entry>,
    mut page: ResMut<Page>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<EntryRoot>>,
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
    let entering = *phase.get() == Phase::GameOver && entry.pending.is_some();
    for n in &mut nodes.p0() {
        set(n, entering);
    }
    for n in &mut nodes.p1() {
        set(n, menu && page.open);
    }
    for n in &mut nodes.p2() {
        set(n, menu && !page.open);
    }
}

fn refresh_entry(
    entry: Res<Entry>,
    mut letters: Query<(&EntryLetter, &mut Text), Without<EntryHeadline>>,
    mut headline: Query<&mut Text, With<EntryHeadline>>,
) {
    if !entry.is_changed() {
        return;
    }
    for (l, mut t) in &mut letters {
        let want = (entry.letters[l.0] as char).to_string();
        if t.0 != want {
            t.0 = want;
        }
    }
    if let (Ok(mut t), Some((map, r))) = (headline.single_mut(), &entry.pending) {
        let secs = r.secs as u32;
        t.0 = format!(
            "{map} · {} kills · {:02}:{:02} · {} {}",
            r.kills,
            secs / 60,
            secs % 60,
            r.class,
            r.difficulty
        );
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
                row.spawn((cell(70.0), Pickable::IGNORE)).with_child(text(
                    f,
                    r.initials.clone(),
                    20.0,
                    GOLD,
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
