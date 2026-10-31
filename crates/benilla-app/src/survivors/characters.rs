//! Saved characters and map completion (`docs/PROGRESSION.md`, phase 2).
//!
//! A character is a name, class, race and gender plus the maps it has completed, one per
//! difficulty; each completion is a talent point, 51 in all. At most [`MAX_CHARACTERS`].
//!
//! The menu opens on **character select**: the list on the left where the create tower stands
//! ([`super::menu::TowerRoot`], hidden), the zone list on the right with a mark per difficulty
//! completed, a difficulty picker above Enter World. **Create** shows the tower again, with a
//! generated name (there is no keyboard on a NativeActivity) and Create / Back where Enter World
//! was ([`super::menu::EnterSlot`]). Delete takes a second tap to confirm.
//!
//! Files, through [`crate::local_state`] so the page's store answers too:
//! `benilla-config/characters/index.txt` (the ids, in order, then `active <id>`) and
//! `characters/<id>.toml` per character.
//!
//! **Progression:** the zones unlock in the list's order ([`menu::zone_order`], easiest first),
//! each once the one before is completed on it; the locked zones leave the list for one line that
//! counts them and names the zone to clear, never what is next. Heroic opens once every zone is
//! completed on Normal, Mythic once every zone is on Heroic ([`Unlocks`]).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::data::{self, Difficulty};
use super::hud::{HudFont, TEXT_FLOOR};
use super::menu::{self, EnterSlot, MenuAction, TowerRoot};
use super::secrets::{self, SecretRace};
use super::{Phase, Rng, Run, Selection};

pub(super) const MAX_CHARACTERS: usize = 8;
/// 16 zones and 3 raids (not yet built) × the difficulties they count on: one point each.
pub(super) const MAX_TALENT_POINTS: usize = 51;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const DIM: Color = Color::srgb(0.55, 0.5, 0.42);
/// A character row's talent count: lighter than [`DIM`] to read at the text floor.
const TALENTS: Color = Color::srgb(0.8, 0.76, 0.66);
const FILL: Color = Color::srgba(0.09, 0.09, 0.09, 0.88);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const BUTTON_HOT: Color = Color::srgba(0.34, 0.22, 0.06, 0.95);
/// An icon's inner 86%: the outer rows are its painted bevel.
const ICON_CROP: [f32; 4] = [0.07, 0.93, 0.07, 0.93];
const PICKED: Color = Color::srgba(0.45, 0.3, 0.05, 0.95);
/// A zone row's completion mark, in UI units: the name line's height.
const MARK: f32 = 18.0;
/// A locked difficulty's label, and the line under it saying what opens it (WoW's red for a
/// requirement not met, lifted to read on the dark button).
const LOCKED_LABEL: Color = Color::srgb(0.72, 0.68, 0.6);
const LOCKED_REASON: Color = Color::srgb(1.0, 0.38, 0.32);
/// A difficulty button's size: two lines, the label and, while locked, why.
const DIFFICULTY_W: f32 = 160.0;
const DIFFICULTY_H: f32 = 50.0;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct Character {
    pub id: u32,
    pub name: String,
    /// `ChrClasses.dbc` id ([`data::ClassDef::id`]): stable where the class list's order is not.
    pub class_id: u8,
    /// A look's race slot: a `ChrRaces` id, or a secret race's look code (`secrets::look_code`),
    /// which [`Self::secret`] overrides on load.
    pub race: u8,
    pub sex: u8,
    /// The secret race played, by key (`secrets::data::SecretDef::key`); `None` for the eight.
    #[serde(default)]
    pub secret: Option<String>,
    /// Which of the race's display variants, among its looks of this sex.
    #[serde(default)]
    pub skin: u8,
    /// Maps completed, as `(map name, difficulty name)`.
    #[serde(default)]
    pub completions: Vec<(String, String)>,
    /// Talent ranks spent, per tree and row (`super::talents`).
    #[serde(default)]
    pub talents: super::talents::Spent,
}

impl Character {
    pub fn talent_points(&self) -> usize {
        self.completions.len().min(MAX_TALENT_POINTS)
    }

    pub fn completed(&self, map: usize, difficulty: Difficulty) -> bool {
        let name = data::MAPS[map].name;
        self.completions
            .iter()
            .any(|(m, d)| m == name && d == difficulty.name())
    }

    fn class_index(&self) -> Option<usize> {
        data::CLASSES.iter().position(|c| c.id == self.class_id)
    }

    /// The race as the list names it.
    pub(super) fn race_label(&self) -> &'static str {
        match &self.secret {
            Some(k) => SecretRace::from_key(k).map_or("Unknown", |r| secrets::def(r).name),
            None => secrets::race_label(self.race),
        }
    }
}

/// The look a saved character wears among its class's `looks`: its secret race by key (a code
/// may have moved since), else its race; the `skin`-th of that race and sex. An unknown secret,
/// or a look that is gone, falls back to the class's first look.
pub(super) fn look_of(c: &Character, looks: &[(u32, u8, u8)]) -> Option<usize> {
    let race = match &c.secret {
        Some(k) => match SecretRace::from_key(k) {
            Some(r) => secrets::look_code(r),
            None => return menu::find(looks, None, None),
        },
        None => c.race,
    };
    let same: Vec<usize> = (0..looks.len())
        .filter(|&i| looks[i].1 == race && looks[i].2 == c.sex)
        .collect();
    same.get(usize::from(c.skin))
        .or(same.first())
        .copied()
        .or_else(|| menu::find(looks, Some(race), None))
        .or_else(|| menu::find(looks, None, None))
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Mode {
    #[default]
    Select,
    Create,
}

#[derive(Resource, Default)]
pub(super) struct Characters {
    pub list: Vec<Character>,
    pub active: Option<u32>,
    mode: Mode,
    /// The name the character being created will take.
    new_name: String,
    /// A first tap on Delete arms it for this character; the second deletes.
    delete_armed: Option<u32>,
    loaded: bool,
    /// Bumped on every change, so the list rebuilds once.
    version: u32,
}

impl Characters {
    fn active(&self) -> Option<&Character> {
        self.active
            .and_then(|id| self.list.iter().find(|c| c.id == id))
    }
}

/// What the active character may pick, worked out from its completions. Each difficulty is a
/// pass through the whole game: Heroic opens once every zone is completed on Normal, Mythic once
/// every zone is on Heroic; within a difficulty the zones open in the list's order, each after
/// the one before is completed on it. The zone list shows the locks of the difficulty selected.
#[derive(Resource)]
pub(super) struct Unlocks {
    /// Per difficulty, per map ([`data::MAPS`] index): open.
    open: [Vec<bool>; 3],
    /// The difficulty the zone list is showing ([`Selection::difficulty`]).
    current: Difficulty,
}

impl Default for Unlocks {
    fn default() -> Self {
        Self::for_character(None, Difficulty::Normal)
    }
}

impl Unlocks {
    /// Open on the difficulty selected.
    pub fn zone(&self, map: usize) -> bool {
        self.zone_on(map, self.current)
    }

    pub fn zone_on(&self, map: usize, d: Difficulty) -> bool {
        self.open[d as usize].get(map).copied().unwrap_or(false)
    }

    /// Whether the difficulty is open at all (its first zone is).
    pub fn difficulty(&self, _map: usize, d: Difficulty) -> bool {
        self.open[d as usize].iter().any(|&o| o)
    }

    /// The zone list's line under the open zones: how many are still locked on the difficulty
    /// shown and what opens the next, never their names. `None` once every zone is open.
    pub fn locked_summary(&self) -> Option<String> {
        let d = self.current;
        let order = menu::zone_order();
        let locked = order.iter().filter(|&&(m, ..)| !self.zone_on(m, d)).count();
        if locked == 0 {
            return None;
        }
        let zones = if locked == 1 { "zone" } else { "zones" };
        Some(
            match order.iter().rev().find(|&&(m, ..)| self.zone_on(m, d)) {
                Some(&(m, ..)) => format!(
                    "{locked} more {zones} locked — clear {} on {} to unlock the next",
                    data::MAPS[m].name,
                    d.name()
                ),
                None => format!(
                    "All {locked} {} {zones} locked — clear every zone on {} first",
                    d.name(),
                    previous(d).unwrap_or(Difficulty::Normal).name()
                ),
            },
        )
    }

    fn for_character(c: Option<&Character>, current: Difficulty) -> Self {
        let done = |m: usize, d: Difficulty| c.is_some_and(|c| c.completed(m, d));
        let order: Vec<usize> = menu::zone_order().into_iter().map(|(m, ..)| m).collect();
        let mut open: [Vec<bool>; 3] = Default::default();
        for d in Difficulty::ALL {
            let mut row = vec![false; data::MAPS.len()];
            let previous_cleared = match d {
                Difficulty::Normal => true,
                Difficulty::Heroic => order.iter().all(|&m| done(m, Difficulty::Normal)),
                Difficulty::Mythic => order.iter().all(|&m| done(m, Difficulty::Heroic)),
            };
            if previous_cleared {
                let mut previous_done = true;
                for &m in &order {
                    if !previous_done {
                        break;
                    }
                    row[m] = true;
                    previous_done = done(m, d);
                }
            }
            open[d as usize] = row;
        }
        Self { open, current }
    }
}

/// The results screen's completion line, placed by `hud::setup_hud`.
#[derive(Component)]
pub(super) struct CompletionLine;

/// The difficulty a pass through every zone on opens this one ([`Unlocks`]).
fn previous(d: Difficulty) -> Option<Difficulty> {
    match d {
        Difficulty::Normal => None,
        Difficulty::Heroic => Some(Difficulty::Normal),
        Difficulty::Mythic => Some(Difficulty::Heroic),
    }
}

#[derive(Component)]
struct CharPanel;
/// The left column's places for the Talents, Records and Secrets openers, in that order under
/// the list; their modules spawn the openers into them (`super::talents`, `super::records`,
/// `super::secrets::page`).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeftSlot {
    Talents,
    Records,
    /// The Secrets page's opener (`super::secrets::page`).
    Secrets,
}
#[derive(Component)]
struct CharRows;
#[derive(Component)]
struct DifficultyBar;
#[derive(Component)]
struct CreateBar;
#[derive(Component)]
struct NewName;
#[derive(Component)]
struct DeleteLabel;
/// A locked difficulty button's second line: what opens it.
#[derive(Component)]
struct LockReason;
#[derive(Component)]
struct CountLabel;
/// A zone row's mark for one difficulty: gold once completed.
#[derive(Component)]
struct ZoneMark(usize, Difficulty);
/// On a zone row once its marks are in.
#[derive(Component)]
struct ZoneMarked;

#[derive(Component, Clone, Copy, PartialEq)]
enum CharButton {
    Pick(u32),
    Create,
    Delete,
    Difficulty(Difficulty),
    Reroll,
    Confirm,
    Back,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Characters>()
        .init_resource::<Unlocks>()
        .add_systems(Startup, spawn_ui.after(super::hud::setup_hud))
        .add_systems(
            Update,
            (
                load,
                press,
                apply_active,
                compute_unlocks,
                rebuild_rows,
                show,
                mark_zones,
                refresh_labels,
            )
                .chain(),
        )
        .add_systems(OnEnter(Phase::GameOver), record_completion);
}

// --- Saving. ---

fn dir() -> Option<std::path::PathBuf> {
    crate::local_state::home().map(|h| h.join("characters"))
}

fn save_index(chars: &Characters) {
    let Some(dir) = dir() else { return };
    let mut text: String = chars.list.iter().map(|c| format!("{}\n", c.id)).collect();
    if let Some(id) = chars.active {
        text.push_str(&format!("active {id}\n"));
    }
    let _ = crate::local_state::write_atomic(&dir.join("index.txt"), &text);
}

pub(super) fn save_character(c: &Character) {
    let Some(dir) = dir() else { return };
    if let Ok(text) = toml::to_string(c) {
        let _ = crate::local_state::write_atomic(&dir.join(format!("{}.toml", c.id)), &text);
    }
}

fn load(mut chars: ResMut<Characters>) {
    if chars.loaded {
        return;
    }
    chars.loaded = true;
    let Some(dir) = dir() else { return };
    let Ok(index) = crate::local_state::read_to_string(&dir.join("index.txt")) else {
        chars.mode = Mode::Create;
        return;
    };
    let mut active = None;
    for line in index.lines().map(str::trim) {
        if let Some(id) = line.strip_prefix("active ") {
            active = id.trim().parse().ok();
            continue;
        }
        let Ok(id) = line.parse::<u32>() else {
            continue;
        };
        let path = dir.join(format!("{id}.toml"));
        if let Some(c) = crate::local_state::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<Character>(&t).ok())
        {
            chars.list.push(c);
        }
    }
    chars.list.truncate(MAX_CHARACTERS);
    chars.active = active.filter(|id| chars.list.iter().any(|c| c.id == *id));
    if chars.active.is_none() {
        chars.active = chars.list.first().map(|c| c.id);
    }
    if chars.list.is_empty() {
        chars.mode = Mode::Create;
    }
    chars.version += 1;
}

// --- Names. ---

const NAME_HEADS: &[&str] = &[
    "Thal", "Mor", "Kael", "Bran", "Gar", "Lor", "Vol", "Jai", "Syl", "Dra", "Ur", "Zul", "Ar",
    "Bel", "Cor", "Fen", "Hal", "Kar", "Mal", "Nar", "Or", "Rha", "Tor", "Vel", "Ash", "Ery",
];
const NAME_TAILS: &[&str] = &[
    "dor", "ion", "ar", "ok", "eth", "an", "is", "ra", "uk", "en", "os", "ira", "gar", "wyn",
    "thas", "ak", "ella", "ius", "om", "ash", "ina", "rek", "dris", "mir",
];

fn random_name(rng: &mut Rng, taken: &[Character]) -> String {
    for _ in 0..32 {
        let name = format!(
            "{}{}",
            NAME_HEADS[rng.index(NAME_HEADS.len())],
            NAME_TAILS[rng.index(NAME_TAILS.len())]
        );
        if !taken.iter().any(|c| c.name == name) {
            return name;
        }
    }
    format!("Hero{}", taken.len() + 1)
}

// --- The screens. ---

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

fn button_node(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    action: CharButton,
    label: &str,
    width: f32,
    extra: impl Bundle,
) {
    parent
        .spawn((
            Button,
            action,
            button_node(width, 40.0),
            BackgroundColor(BUTTON),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            b.spawn((text(font, label, 16.0, Color::WHITE), extra));
        });
}

/// The three panels, empty and out of the layout until the menu shows them.
pub(super) fn spawn_ui(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
    // The left column, where the create tower stands: the list (which takes what is left), then
    // the Talents and Records slots ([`LeftSlot`]), one flow from under the WoW logo down to Enter
    // World's line, so no part can sit over another at any aspect. Packed to the bottom: on
    // create, with the list out, Records stays under the tower.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(28.0),
                top: Val::Px(136.0),
                bottom: Val::Px(menu::ENTER_BOTTOM),
                width: Val::Px(236.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|col| {
            spawn_list(col, f);
            col.spawn((LeftSlot::Talents, Node::default(), Pickable::IGNORE));
            col.spawn((LeftSlot::Records, Node::default(), Pickable::IGNORE));
            col.spawn((LeftSlot::Secrets, Node::default(), Pickable::IGNORE));
        });
    spawn_bars(&mut commands, f);
}

/// The character list, filling the left column above Talents and Records. It clips rather than
/// spills: on the 768-unit canvas the rows get ~350 units, eight two-line rows ~350.
fn spawn_list(col: &mut ChildSpawnerCommands, f: &Handle<Font>) {
    col.spawn((
        CharPanel,
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            min_height: Val::Px(0.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(8.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            overflow: Overflow::clip_y(),
            display: Display::None,
            ..default()
        },
        BackgroundColor(FILL),
        BorderColor::all(EDGE),
    ))
    .with_children(|p| {
        p.spawn((text(f, "Characters", 18.0, GOLD), CountLabel));
        // The rows take what Create and Delete leave, and clip past it.
        p.spawn((
            CharRows,
            Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                min_height: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                overflow: Overflow::clip_y(),
                ..default()
            },
        ));
        spawn_button(p, f, CharButton::Create, "Create New Character", 210.0, ());
        spawn_button(
            p,
            f,
            CharButton::Delete,
            "Delete Character",
            210.0,
            DeleteLabel,
        );
    });
}

/// The difficulty picker and the create bar, over the bottom centre.
fn spawn_bars(commands: &mut Commands, f: &Handle<Font>) {
    // The difficulty picker, above the character's name line and Enter World.
    commands
        .spawn((
            DifficultyBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(menu::DIFFICULTY_BOTTOM),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(8.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            for d in Difficulty::ALL {
                let label = format!("{} · {} min", d.name(), d.run_secs() as u32 / 60);
                bar.spawn((
                    Button,
                    CharButton::Difficulty(d),
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..button_node(DIFFICULTY_W, DIFFICULTY_H)
                    },
                    BackgroundColor(BUTTON),
                    BorderColor::all(EDGE),
                ))
                .with_children(|b| {
                    b.spawn(text(f, label, 16.0, Color::WHITE));
                    // While locked: what opens it ([`refresh_labels`]).
                    b.spawn((
                        text(f, "", TEXT_FLOOR, LOCKED_REASON),
                        LockReason,
                        Node {
                            display: Display::None,
                            ..default()
                        },
                    ));
                });
            }
        });
    // Creating: the name, a new one, Create and Back, where Enter World stands.
    commands
        .spawn((
            CreateBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(menu::ENTER_BOTTOM),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((text(f, "", 24.0, GOLD), NewName));
            bar.spawn((
                Node {
                    column_gap: Val::Px(8.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                spawn_button(row, f, CharButton::Back, "Back", 120.0, ());
                spawn_button(row, f, CharButton::Reroll, "New name", 140.0, ());
                spawn_button(row, f, CharButton::Confirm, "Create", 160.0, ());
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn press(
    mut buttons: Query<(&Interaction, &CharButton, &mut BackgroundColor), Changed<Interaction>>,
    mut chars: ResMut<Characters>,
    mut selection: ResMut<Selection>,
    mut rng: ResMut<Rng>,
    creatures: Option<Res<crate::entities::Creatures>>,
    unlocks: Res<Unlocks>,
) {
    for (interaction, action, mut bg) in &mut buttons {
        if !matches!(action, CharButton::Pick(_) | CharButton::Difficulty(_)) {
            bg.0 = match interaction {
                Interaction::None => BUTTON,
                _ => BUTTON_HOT,
            };
        }
        if *interaction != Interaction::Pressed {
            continue;
        }
        if *action != CharButton::Delete {
            chars.delete_armed = None;
        }
        match *action {
            CharButton::Pick(id) => {
                chars.active = Some(id);
                save_index(&chars);
            }
            CharButton::Create if chars.list.len() < MAX_CHARACTERS => {
                chars.mode = Mode::Create;
                chars.new_name = random_name(&mut rng, &chars.list);
            }
            CharButton::Create => {}
            CharButton::Delete => {
                let Some(id) = chars.active else { continue };
                if chars.delete_armed == Some(id) {
                    chars.list.retain(|c| c.id != id);
                    chars.active = chars.list.first().map(|c| c.id);
                    chars.delete_armed = None;
                    selection.character = None;
                    if chars.list.is_empty() {
                        chars.mode = Mode::Create;
                        chars.new_name = random_name(&mut rng, &chars.list);
                    }
                    save_index(&chars);
                } else {
                    chars.delete_armed = Some(id);
                }
            }
            CharButton::Difficulty(d) if !unlocks.difficulty(selection.map, d) => {}
            CharButton::Difficulty(d) => {
                selection.difficulty = d;
                selection.version += 1;
            }
            CharButton::Reroll => chars.new_name = random_name(&mut rng, &chars.list),
            CharButton::Back if !chars.list.is_empty() => chars.mode = Mode::Select,
            CharButton::Back => {}
            CharButton::Confirm => {
                let (Some(class), Some(&(_, race, sex))) = (
                    selection.class_def(),
                    selection.look.and_then(|i| selection.looks.get(i)),
                ) else {
                    continue;
                };
                if chars.list.len() >= MAX_CHARACTERS {
                    continue;
                }
                let id = chars.list.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                if chars.new_name.is_empty() {
                    chars.new_name = random_name(&mut rng, &chars.list);
                }
                let looks = &selection.looks;
                let skin = selection.look.map_or(0, |i| {
                    (0..i)
                        .filter(|&j| looks[j].1 == race && looks[j].2 == sex)
                        .count()
                });
                let c = Character {
                    id,
                    name: chars.new_name.clone(),
                    class_id: class.id,
                    race,
                    sex,
                    secret: secrets::from_look_code(race).map(|r| secrets::def(r).key.to_string()),
                    skin: u8::try_from(skin).unwrap_or(0),
                    completions: Vec::new(),
                    talents: Default::default(),
                };
                save_character(&c);
                chars.list.push(c);
                chars.active = Some(id);
                chars.mode = Mode::Select;
                save_index(&chars);
            }
        }
        chars.version += 1;
    }
    // A fresh install opens on Create with a name already rolled.
    if chars.mode == Mode::Create && chars.new_name.is_empty() && creatures.is_some() {
        chars.new_name = random_name(&mut rng, &chars.list);
        chars.version += 1;
    }
}

/// On select, the active character dresses the menu hero: its class, race and gender.
fn apply_active(
    chars: Res<Characters>,
    mut selection: ResMut<Selection>,
    creatures: Option<Res<crate::entities::Creatures>>,
    progress: Res<secrets::Progress>,
) {
    let Some(creatures) = creatures.as_deref() else {
        return;
    };
    if chars.mode != Mode::Select {
        if selection.character.is_some() {
            selection.character = None;
        }
        return;
    }
    let Some(c) = chars.active() else { return };
    if selection.character == Some(c.id) {
        return;
    }
    let Some(class) = c.class_index() else { return };
    // The character's own secret race is its own, even if the account's progress was lost.
    let mut unlocked = progress.unlocked.clone();
    if let Some(k) = c.secret.as_ref().filter(|k| !unlocked.contains(k)) {
        unlocked.push(k.clone());
    }
    let all = menu::all_looks(creatures, &unlocked);
    if let Some(look) = look_of(c, &all[class]) {
        menu::set(&mut selection, &all, class, look);
        selection.character = Some(c.id);
    }
}

fn rebuild_rows(
    mut commands: Commands,
    chars: Res<Characters>,
    font: Option<Res<HudFont>>,
    rows: Query<Entity, With<CharRows>>,
    mut built: Local<Option<u32>>,
) {
    if *built == Some(chars.version) {
        return;
    }
    let (Some(font), Ok(list)) = (font, rows.single()) else {
        return;
    };
    *built = Some(chars.version);
    let f = &font.0;
    commands.entity(list).despawn_related::<Children>();
    commands.entity(list).with_children(|ui| {
        for c in &chars.list {
            let picked = chars.active == Some(c.id);
            let class = c.class_index().map(|i| &data::CLASSES[i]);
            let class_name = class.map_or("", |k| k.name);
            let class_color = class.map_or(Color::WHITE, |k| super::hud::rgb(k.color));
            ui.spawn((
                Button,
                CharButton::Pick(c.id),
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0.0,
                    // Tight enough that the eighth (last allowed) character's row fits the list
                    // at the A11 text sizes (review, 2026-10-31).
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(if picked { PICKED } else { Color::NONE }),
                BorderColor::all(if picked { EDGE } else { Color::NONE }),
            ))
            .with_children(|row| {
                // The name, its talent points at the line's end; the race and class under it.
                row.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Baseline,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|line| {
                    line.spawn(text(f, c.name.clone(), 16.0, GOLD));
                    line.spawn(text(
                        f,
                        format!("{}/{} talents", c.talent_points(), MAX_TALENT_POINTS),
                        TEXT_FLOOR,
                        TALENTS,
                    ));
                });
                row.spawn(text(
                    f,
                    format!("{} {}", c.race_label(), class_name),
                    14.0,
                    class_color,
                ));
            });
        }
    });
}

/// Which screen is up: select (list, difficulties, Enter World) or create (tower, name, Create).
#[allow(clippy::type_complexity)]
fn show(
    phase: Res<State<Phase>>,
    chars: Res<Characters>,
    mut panels: ParamSet<(
        Query<&mut Node, With<CharPanel>>,
        Query<&mut Node, With<DifficultyBar>>,
        Query<&mut Node, With<CreateBar>>,
        Query<&mut Node, With<TowerRoot>>,
        Query<&mut Node, With<EnterSlot>>,
    )>,
) {
    let menu = *phase.get() == Phase::Menu;
    let select = menu && chars.mode == Mode::Select;
    let create = menu && chars.mode == Mode::Create;
    let set = |mut node: Mut<Node>, on: bool| {
        let display = if on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    };
    for n in &mut panels.p0() {
        set(n, select);
    }
    for n in &mut panels.p1() {
        set(n, select);
    }
    for n in &mut panels.p2() {
        set(n, create);
    }
    // The menu's own panels: off only while the other screen needs their place.
    for n in &mut panels.p3() {
        set(n, !select);
    }
    for n in &mut panels.p4() {
        set(n, !create);
    }
}

/// Three marks on every zone row, N H M, lit where the active character completed it, beside the
/// name (the line under it runs the row's width).
fn mark_zones(
    mut commands: Commands,
    rows: Query<(Entity, &MenuAction), (With<Button>, Without<ZoneMarked>)>,
) {
    for (e, action) in &rows {
        let MenuAction::Map(i) = *action else {
            continue;
        };
        commands.entity(e).insert(ZoneMarked).with_children(|row| {
            row.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(2.0),
                    top: Val::Px(0.0),
                    column_gap: Val::Px(3.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|marks| {
                for d in Difficulty::ALL {
                    marks.spawn((
                        Node {
                            width: Val::Px(MARK),
                            height: Val::Px(MARK),
                            ..default()
                        },
                        ImageNode {
                            color: Color::NONE,
                            ..default()
                        },
                        super::hud::UiArt::crop(d.icon(), ICON_CROP),
                        ZoneMark(i, d),
                        Pickable::IGNORE,
                    ));
                }
            });
        });
    }
}

#[allow(clippy::type_complexity)]
fn refresh_labels(
    chars: Res<Characters>,
    selection: Res<Selection>,
    mut marks: Query<(&ZoneMark, &mut ImageNode)>,
    fresh: Query<(), (With<ZoneMark>, Added<super::hud::ArtDone>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<NewName>>,
        Query<&mut Text, With<DeleteLabel>>,
        Query<&mut Text, With<CountLabel>>,
    )>,
    mut difficulty: Query<(
        &CharButton,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut zone_rows: Query<(&MenuAction, &mut Node), (With<Button>, Without<LockReason>)>,
    mut summary: Query<
        (&mut Text, &mut Node),
        (
            With<menu::ZoneSummary>,
            Without<MenuAction>,
            Without<LockReason>,
            Without<DeleteLabel>,
            Without<NewName>,
            Without<CountLabel>,
        ),
    >,
    mut reasons: Query<(&mut Node, &mut Text), (With<LockReason>, Without<MenuAction>)>,
    mut label_colors: Query<&mut TextColor, (Without<ZoneMark>, Without<LockReason>)>,
    unlocks: Res<Unlocks>,
) {
    if !chars.is_changed() && !selection.is_changed() && !unlocks.is_changed() && fresh.is_empty() {
        return;
    }
    // A locked zone leaves the list; one line under the open ones counts them.
    for (action, mut node) in &mut zone_rows {
        let MenuAction::Map(i) = *action else {
            continue;
        };
        let display = if unlocks.zone(i) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok((mut text, mut node)) = summary.single_mut() {
        let line = unlocks.locked_summary();
        let display = if line.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let line = line.unwrap_or_default();
        if text.0 != line {
            text.0 = line;
        }
    }
    let active = chars.active();
    for (mark, mut image) in &mut marks {
        let done = active.is_some_and(|c| c.completed(mark.0, mark.1));
        // Lit once completed; until then dark and faint, the icon only a hint.
        let want = if done {
            Color::WHITE
        } else {
            Color::srgba(0.3, 0.3, 0.3, 0.55)
        };
        if image.color != want {
            image.color = want;
        }
    }
    if let Ok(mut t) = texts.p0().single_mut() {
        t.0 = chars.new_name.clone();
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        t.0 = match (chars.delete_armed, active) {
            (Some(_), Some(c)) => format!("Tap again: delete {}", c.name),
            _ => "Delete Character".into(),
        };
    }
    if let Ok(mut t) = texts.p2().single_mut() {
        t.0 = format!("Characters {}/{}", chars.list.len(), MAX_CHARACTERS);
    }
    for (action, mut bg, mut border, children) in &mut difficulty {
        let CharButton::Difficulty(d) = *action else {
            continue;
        };
        let open = unlocks.difficulty(selection.map, d);
        let on = selection.difficulty == d;
        bg.0 = if on { PICKED } else { BUTTON };
        *border = BorderColor::all(match (on, open) {
            (true, _) => GOLD,
            (false, true) => EDGE,
            (false, false) => DIM,
        });
        for child in children.iter() {
            if let Ok(mut color) = label_colors.get_mut(child) {
                color.0 = if open { Color::WHITE } else { LOCKED_LABEL };
            }
            if let Ok((mut node, mut text)) = reasons.get_mut(child) {
                let display = if open { Display::None } else { Display::Flex };
                if node.display != display {
                    node.display = display;
                }
                let why =
                    previous(d).map_or(String::new(), |p| format!("Clear all {} zones", p.name()));
                if text.0 != why {
                    text.0 = why;
                }
            }
        }
    }
}

/// The active character's unlocks; a pick that is no longer open falls back to what is: the
/// furthest open zone, Normal.
fn compute_unlocks(
    chars: Res<Characters>,
    mut unlocks: ResMut<Unlocks>,
    mut selection: ResMut<Selection>,
    mut seen: Local<Option<(Option<u32>, u32, Difficulty)>>,
    auto: Option<Res<super::auto::Auto>>,
) {
    // The auto-player picks its own zone and difficulty, past any character's locks.
    if auto.is_some() {
        return;
    }
    if !unlocks.difficulty(selection.map, selection.difficulty) {
        selection.difficulty = Difficulty::Normal;
        selection.version += 1;
    }
    let key = (chars.active, chars.version, selection.difficulty);
    if *seen != Some(key) {
        *seen = Some(key);
        *unlocks = Unlocks::for_character(chars.active(), selection.difficulty);
    }
    if !unlocks.zone(selection.map) {
        if let Some(m) = menu::zone_order()
            .into_iter()
            .map(|(m, ..)| m)
            .filter(|&m| unlocks.zone(m))
            .last()
        {
            selection.map = m;
            selection.version += 1;
        }
    }
}

// --- Completion. ---

/// A won run completes its map on its difficulty for the character playing; a first completion
/// is a talent point.
fn record_completion(
    run: Res<Run>,
    selection: Res<Selection>,
    mut chars: ResMut<Characters>,
    mut lines: Query<&mut Text, With<CompletionLine>>,
    auto: Option<Res<super::auto::Auto>>,
) {
    let mut line = String::new();
    // An unattended or god-mode run never completes a map for the character on the menu.
    if run.won && super::secrets::records_progress(auto.is_some(), super::hero::god_mode()) {
        let difficulty = data::difficulty();
        let map = data::MAPS[selection.stage.min(data::MAPS.len() - 1)].name;
        if let Some(c) = selection
            .character
            .and_then(|id| chars.list.iter_mut().find(|c| c.id == id))
        {
            let first = !c
                .completions
                .iter()
                .any(|(m, d)| m == map && d == difficulty.name());
            super::combatlog::event(
                run.elapsed,
                format!(
                    "completion: {} on {} for {} ({})",
                    map,
                    difficulty.name(),
                    c.name,
                    if first {
                        "first, talent point"
                    } else {
                        "repeat"
                    }
                ),
            );
            if first {
                c.completions
                    .push((map.to_string(), difficulty.name().to_string()));
                save_character(c);
            }
            line = if first {
                format!(
                    "{map} completed on {}! Talent point earned: {}/{}",
                    difficulty.name(),
                    c.talent_points(),
                    MAX_TALENT_POINTS
                )
            } else {
                format!("{map} completed on {} again.", difficulty.name())
            };
        }
        chars.version += 1;
    }
    for mut t in &mut lines {
        t.0 = line.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(completions: &[(usize, Difficulty)]) -> Character {
        Character {
            id: 1,
            name: "Test".into(),
            class_id: 1,
            race: 1,
            sex: 0,
            secret: None,
            skin: 0,
            completions: completions
                .iter()
                .map(|&(m, d)| (data::MAPS[m].name.to_string(), d.name().to_string()))
                .collect(),
            talents: Default::default(),
        }
    }

    /// The zone list counts the locked zones and names the one to clear, not the next.
    #[test]
    fn the_locked_summary_names_the_zone_to_clear() {
        let order: Vec<usize> = menu::zone_order().into_iter().map(|(m, ..)| m).collect();
        let n = order.len();
        let fresh = Unlocks::for_character(None, Difficulty::Normal);
        assert_eq!(
            fresh.locked_summary().unwrap(),
            format!(
                "{} more zones locked — clear {} on Normal to unlock the next",
                n - 1,
                data::MAPS[order[0]].name
            )
        );
        let one = hero(&[(order[0], Difficulty::Normal)]);
        let s = Unlocks::for_character(Some(&one), Difficulty::Normal)
            .locked_summary()
            .unwrap();
        assert!(s.starts_with(&format!("{} more zones", n - 2)), "{s}");
        assert!(s.contains(data::MAPS[order[1]].name), "{s}");
        assert!(!s.contains(data::MAPS[order[2]].name), "{s}");
    }

    #[test]
    fn the_locked_summary_goes_once_all_are_open() {
        let order: Vec<usize> = menu::zone_order().into_iter().map(|(m, ..)| m).collect();
        let all: Vec<_> = order.iter().map(|&m| (m, Difficulty::Normal)).collect();
        let c = hero(&all);
        assert_eq!(
            Unlocks::for_character(Some(&c), Difficulty::Normal).locked_summary(),
            None
        );
        let heroic = Unlocks::for_character(Some(&c), Difficulty::Heroic)
            .locked_summary()
            .unwrap();
        assert!(heroic.ends_with("on Heroic to unlock the next"), "{heroic}");
        let mythic = Unlocks::for_character(Some(&c), Difficulty::Mythic)
            .locked_summary()
            .unwrap();
        assert_eq!(
            mythic,
            format!(
                "All {} Mythic zones locked — clear every zone on Heroic first",
                order.len()
            )
        );
    }
}
