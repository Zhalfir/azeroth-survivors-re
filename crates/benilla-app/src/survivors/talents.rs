//! Talents and tier sets: the power a character carries between runs (`docs/PROGRESSION.md`).
//!
//! **Talents.** Every class has its three WoW trees, each with a focus — offense (damage, crit,
//! haste), defense (health, armour, regeneration) or utility (area, speed, pickup reach,
//! experience). A tree is five rows: four talents of five ranks and a one-rank capstone named for
//! the tree's 31-point talent; row `r` opens at `5 × r` points in the tree, as in WoW. 21 points a
//! tree, 63 a class, against the 51 a character can earn: a build is a choice. Points come from
//! map completions ([`super::characters::Character::talent_points`]).
//!
//! **Tier sets.** Completing every zone on a difficulty unlocks that tier's set: Normal T1, Heroic
//! T2, Mythic T3. The best one unlocked is worn and adds its stats.
//!
//! Both fold into [`Bonus`], which `Run::stats` applies after the run's own passives.
//! `WOW_SURVIVORS_TALENTS=<n>` and `WOW_SURVIVORS_TIER=<0..3>` give an unattended run
//! (`super::auto`) a character's power without one, for balance testing.

use bevy::prelude::*;

use super::characters::Characters;
use super::data::{self, Difficulty, Stat};
use super::hud::{HudFont, SpellIcon};
use super::{Phase, Run, Selection};

/// Talent ranks a tree's rows hold, the last the capstone.
pub(super) const ROW_RANKS: [u8; 5] = [5, 5, 5, 5, 1];
/// Points spent in a tree to open row `r`.
pub(super) const ROW_GATE: u8 = 5;

/// A tree's focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Focus {
    Offense,
    Defense,
    Utility,
}

/// One row's talent: a name, its icon's spell, the stat it raises and by how much a rank.
#[derive(Clone, Copy, Debug)]
pub(super) struct Talent {
    pub name: &'static str,
    pub spell: u32,
    pub stat: Stat,
    pub per_rank: f32,
}

/// The four ranked rows of a focus; the capstone is the class tree's own ([`CAPSTONES`]).
fn rows(focus: Focus) -> [Talent; 4] {
    let t = |name, spell, stat, per_rank| Talent {
        name,
        spell,
        stat,
        per_rank,
    };
    // Icons borrow the level-up passives' spells: the same stat, the same picture.
    match focus {
        Focus::Offense => [
            t("Brute Force", 19740, Stat::Might, 0.03),
            t("Precision", 20217, Stat::Crit, 0.01),
            t("Flurry", 19742, Stat::Haste, 0.02),
            t("Ferocity", 19740, Stat::Might, 0.03),
        ],
        Focus::Defense => [
            t("Toughness", 1243, Stat::MaxHp, 0.04),
            t("Thick Hide", 1126, Stat::Armor, 0.4),
            t("Second Wind", 139, Stat::Regen, 0.2),
            t("Endurance", 1243, Stat::MaxHp, 0.04),
        ],
        Focus::Utility => [
            t("Improved Range", 1008, Stat::Area, 0.03),
            t("Pathfinding", 13159, Stat::Speed, 0.02),
            t("Keen Senses", 132, Stat::Magnet, 0.08),
            t("Studied Lore", 1459, Stat::Growth, 0.03),
        ],
    }
}

/// A capstone's stat, by focus: a big step of the focus's main stat.
fn capstone_stat(focus: Focus) -> (Stat, f32) {
    match focus {
        Focus::Offense => (Stat::Might, 0.12),
        Focus::Defense => (Stat::MaxHp, 0.15),
        Focus::Utility => (Stat::Haste, 0.10),
    }
}

/// Every class's trees: `(class id, [(tree name, focus, capstone name)])`, Vanilla's trees and
/// 31-point talents.
pub(super) static TREES: &[(u8, [(&str, Focus, &str); 3])] = &[
    (
        1,
        [
            ("Arms", Focus::Offense, "Mortal Strike"),
            ("Fury", Focus::Utility, "Bloodthirst"),
            ("Protection", Focus::Defense, "Shield Slam"),
        ],
    ),
    (
        2,
        [
            ("Holy", Focus::Utility, "Holy Shock"),
            ("Protection", Focus::Defense, "Holy Shield"),
            ("Retribution", Focus::Offense, "Repentance"),
        ],
    ),
    (
        3,
        [
            ("Beast Mastery", Focus::Utility, "Bestial Wrath"),
            ("Marksmanship", Focus::Offense, "Trueshot Aura"),
            ("Survival", Focus::Defense, "Wyvern Sting"),
        ],
    ),
    (
        4,
        [
            ("Assassination", Focus::Offense, "Vigor"),
            ("Combat", Focus::Utility, "Adrenaline Rush"),
            ("Subtlety", Focus::Defense, "Preparation"),
        ],
    ),
    (
        5,
        [
            ("Discipline", Focus::Defense, "Power Infusion"),
            ("Holy", Focus::Utility, "Spirit of Redemption"),
            ("Shadow", Focus::Offense, "Shadowform"),
        ],
    ),
    (
        7,
        [
            ("Elemental", Focus::Offense, "Elemental Mastery"),
            ("Enhancement", Focus::Utility, "Stormstrike"),
            ("Restoration", Focus::Defense, "Mana Tide Totem"),
        ],
    ),
    (
        8,
        [
            ("Arcane", Focus::Utility, "Arcane Power"),
            ("Fire", Focus::Offense, "Combustion"),
            ("Frost", Focus::Defense, "Ice Barrier"),
        ],
    ),
    (
        9,
        [
            ("Affliction", Focus::Utility, "Dark Pact"),
            ("Demonology", Focus::Defense, "Soul Link"),
            ("Destruction", Focus::Offense, "Conflagrate"),
        ],
    ),
    (
        11,
        [
            ("Balance", Focus::Offense, "Moonkin Form"),
            ("Feral Combat", Focus::Utility, "Leader of the Pack"),
            ("Restoration", Focus::Defense, "Swiftmend"),
        ],
    ),
];

pub(super) fn trees_of(class_id: u8) -> [(&'static str, Focus, &'static str); 3] {
    TREES
        .iter()
        .find(|(id, _)| *id == class_id)
        .map_or(TREES[0].1, |(_, t)| *t)
}

/// Ranks spent, per tree and row.
pub(super) type Spent = [[u8; 5]; 3];

pub(super) fn spent_total(spent: &Spent) -> usize {
    spent.iter().flatten().map(|&r| usize::from(r)).sum()
}

fn tree_total(row: &[u8; 5]) -> u8 {
    row.iter().sum()
}

/// Whether one more rank fits: the row's cap, its gate, and the points left.
pub(super) fn can_add(spent: &Spent, tree: usize, row: usize, points: usize) -> bool {
    spent_total(spent) < points
        && spent[tree][row] < ROW_RANKS[row]
        && tree_total(&spent[tree]) >= ROW_GATE * row as u8
}

/// What talents and a tier set add to a run's stats.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Bonus {
    pub might: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub regen: f32,
    pub haste: f32,
    pub area: f32,
    pub magnet: f32,
    pub growth: f32,
    pub armor: f32,
    pub crit: f32,
}

impl Bonus {
    fn add(&mut self, stat: Stat, v: f32) {
        match stat {
            Stat::Might => self.might += v,
            Stat::MaxHp => self.max_hp += v,
            Stat::Speed => self.speed += v,
            Stat::Regen => self.regen += v,
            Stat::Haste => self.haste += v,
            Stat::Area => self.area += v,
            Stat::Magnet => self.magnet += v,
            Stat::Growth => self.growth += v,
            Stat::Armor => self.armor += v,
            Stat::Crit => self.crit += v,
        }
    }

    pub fn from_talents(class_id: u8, spent: &Spent) -> Self {
        let mut b = Self::default();
        for (t, (_, focus, _)) in trees_of(class_id).iter().enumerate() {
            for (r, talent) in rows(*focus).iter().enumerate() {
                b.add(talent.stat, talent.per_rank * f32::from(spent[t][r]));
            }
            if spent[t][4] > 0 {
                let (stat, v) = capstone_stat(*focus);
                b.add(stat, v);
            }
        }
        b
    }

    /// A tier set's stats; `tier` 0 is none.
    pub fn tier(tier: u8) -> Self {
        let mut b = Self::default();
        match tier {
            1 => {
                b.might = 0.10;
                b.max_hp = 0.10;
            }
            2 => {
                b.might = 0.20;
                b.max_hp = 0.20;
                b.armor = 1.0;
            }
            3 => {
                b.might = 0.30;
                b.max_hp = 0.30;
                b.armor = 2.0;
                b.crit = 0.05;
            }
            _ => {}
        }
        b
    }

    pub fn plus(mut self, o: Bonus) -> Self {
        self.might += o.might;
        self.max_hp += o.max_hp;
        self.speed += o.speed;
        self.regen += o.regen;
        self.haste += o.haste;
        self.area += o.area;
        self.magnet += o.magnet;
        self.growth += o.growth;
        self.armor += o.armor;
        self.crit += o.crit;
        self
    }
}

/// The tier set a character has earned: the hardest difficulty with every zone completed.
pub(super) fn tier_of(c: &super::characters::Character) -> u8 {
    let mut tier = 0;
    for (i, d) in Difficulty::ALL.into_iter().enumerate() {
        if (0..data::MAPS.len()).all(|m| c.completed(m, d)) {
            tier = i as u8 + 1;
        }
    }
    tier
}

/// `n` points as an unattended run spends them: offense, then defense, then utility, each row
/// in turn.
pub(super) fn auto_spend(class_id: u8, n: usize) -> Spent {
    let mut spent = Spent::default();
    let trees = trees_of(class_id);
    let order = [Focus::Offense, Focus::Defense, Focus::Utility];
    for focus in order {
        let Some(t) = trees.iter().position(|(_, f, _)| *f == focus) else {
            continue;
        };
        for r in 0..5 {
            while can_add(&spent, t, r, n) {
                spent[t][r] += 1;
            }
        }
    }
    spent
}

fn env_usize(key: &str) -> Option<usize> {
    crate::webenv::var(key).and_then(|v| v.trim().parse().ok())
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<TalentPage>()
        .add_systems(Startup, spawn_page.after(super::hud::setup_hud))
        .add_systems(OnEnter(Phase::Playing), apply_bonus)
        .add_systems(Update, (press, show, rebuild).chain());
}

/// Fold the character's talents and tier into the run, each time play (re)starts; a fresh run
/// also fills the larger health pool.
fn apply_bonus(mut run: ResMut<Run>, selection: Res<Selection>, chars: Res<Characters>) {
    let character = selection
        .character
        .and_then(|id| chars.list.iter().find(|c| c.id == id));
    let class_id = run.class.id;
    let (spent, tier) = match character {
        Some(c) => (c.talents, tier_of(c)),
        None => (
            auto_spend(class_id, env_usize("WOW_SURVIVORS_TALENTS").unwrap_or(0)),
            env_usize("WOW_SURVIVORS_TIER").unwrap_or(0).min(3) as u8,
        ),
    };
    let bonus = Bonus::from_talents(class_id, &spent).plus(Bonus::tier(tier));
    if run.bonus != bonus {
        let fresh = run.elapsed < 0.5;
        run.bonus = bonus;
        if fresh {
            run.hp = run.stats().max_hp;
            super::combatlog::event(
                run.elapsed,
                format!(
                    "talents: {} points, tier {tier}; bonus {bonus:?}",
                    spent_total(&spent)
                ),
            );
        }
    }
}

// --- The talent page. ---

#[derive(Resource, Default)]
struct TalentPage {
    open: bool,
    version: u32,
}

#[derive(Component)]
struct PageRoot;
#[derive(Component)]
struct PageBody;
#[derive(Component)]
pub(super) struct OpenTalents;

#[derive(Component, Clone, Copy, PartialEq)]
enum TalentButton {
    Open,
    Add(usize, usize),
    Reset,
    Close,
}

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const DIM: Color = Color::srgb(0.55, 0.5, 0.42);
const FILL: Color = Color::srgba(0.03, 0.025, 0.02, 0.97);
const EDGE: Color = Color::srgba(0.8, 0.62, 0.25, 0.9);
const BUTTON: Color = Color::srgba(0.16, 0.11, 0.04, 0.95);
const GREEN: Color = Color::srgb(0.2, 1.0, 0.2);

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

fn button_bundle(action: TalentButton, w: f32, h: f32) -> impl Bundle {
    (
        Button,
        action,
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    )
}

fn spawn_page(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
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
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.94)),
            GlobalZIndex(125),
        ))
        .with_child((
            PageBody,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
        ));
    // Its button, under Records on the character screen.
    commands
        .spawn((
            OpenTalents,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(28.0),
                bottom: Val::Px(84.0),
                width: Val::Px(236.0),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(50),
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn(button_bundle(TalentButton::Open, 236.0, 44.0))
                .with_child(text(f, "Talents", 18.0, Color::WHITE));
        });
}

fn press(
    buttons: Query<(&Interaction, &TalentButton), Changed<Interaction>>,
    mut page: ResMut<TalentPage>,
    mut chars: ResMut<Characters>,
    selection: Res<Selection>,
) {
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *action {
            TalentButton::Open => page.open = true,
            TalentButton::Close => page.open = false,
            TalentButton::Reset | TalentButton::Add(..) => {
                let Some(c) = selection
                    .character
                    .and_then(|id| chars.list.iter_mut().find(|c| c.id == id))
                else {
                    continue;
                };
                match *action {
                    TalentButton::Reset => c.talents = Spent::default(),
                    TalentButton::Add(t, r) => {
                        if can_add(&c.talents, t, r, c.talent_points()) {
                            c.talents[t][r] += 1;
                        }
                    }
                    _ => {}
                }
                super::characters::save_character(c);
            }
        }
        page.version += 1;
    }
}

fn show(
    phase: Res<State<Phase>>,
    mut page: ResMut<TalentPage>,
    selection: Res<Selection>,
    mut roots: Query<&mut Node, (With<PageRoot>, Without<OpenTalents>)>,
    mut opener: Query<&mut Node, (With<OpenTalents>, Without<PageRoot>)>,
) {
    let menu = *phase.get() == Phase::Menu && selection.character.is_some();
    if !menu && page.open {
        page.open = false;
    }
    let set = |n: &mut Node, on: bool| {
        let d = if on { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    };
    for mut n in &mut roots {
        set(&mut n, menu && page.open);
    }
    for mut n in &mut opener {
        set(&mut n, menu && !page.open);
    }
}

fn rebuild(
    mut commands: Commands,
    page: Res<TalentPage>,
    chars: Res<Characters>,
    selection: Res<Selection>,
    font: Option<Res<HudFont>>,
    body: Query<Entity, With<PageBody>>,
    mut built: Local<Option<(u32, Option<u32>)>>,
) {
    let key = (page.version, selection.character);
    if !page.open || *built == Some(key) {
        return;
    }
    let (Some(font), Ok(body)) = (font, body.single()) else {
        return;
    };
    let Some(c) = selection
        .character
        .and_then(|id| chars.list.iter().find(|c| c.id == id))
    else {
        return;
    };
    *built = Some(key);
    let f = &font.0;
    let points = c.talent_points();
    let left = points.saturating_sub(spent_total(&c.talents));
    let tier = tier_of(c);
    commands.entity(body).despawn_related::<Children>();
    commands.entity(body).with_children(|ui| {
        ui.spawn(text(f, format!("{} — Talents", c.name), 34.0, GOLD));
        ui.spawn(text(
            f,
            format!(
                "Points: {left} free of {points} earned · Tier set: {}",
                match tier {
                    0 => "none yet".to_string(),
                    t => format!("T{t}"),
                }
            ),
            16.0,
            Color::WHITE,
        ));
        ui.spawn(Node {
            column_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|cols| {
            for (t, (tree, focus, capstone)) in trees_of(c.class_id).iter().enumerate() {
                let total = tree_total(&c.talents[t]);
                cols.spawn((
                    Node {
                        width: Val::Px(250.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(FILL),
                    BorderColor::all(EDGE),
                ))
                .with_children(|col| {
                    col.spawn(text(f, format!("{tree} ({total})"), 20.0, GOLD));
                    let talents = rows(*focus);
                    for r in 0..5 {
                        let (name, spell, max) = if r < 4 {
                            (talents[r].name, talents[r].spell, ROW_RANKS[r])
                        } else {
                            let start = data::CLASSES
                                .iter()
                                .find(|k| k.id == c.class_id)
                                .map_or(0, |k| data::ability(k.start).spell);
                            (*capstone, start, 1)
                        };
                        let have = c.talents[t][r];
                        let open = total >= ROW_GATE * r as u8;
                        let addable = can_add(&c.talents, t, r, points);
                        col.spawn((
                            button_bundle(TalentButton::Add(t, r), 228.0, 46.0),
                            BorderColor::all(if have > 0 {
                                GREEN
                            } else if open {
                                EDGE
                            } else {
                                DIM.with_alpha(0.4)
                            }),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Node {
                                    width: Val::Px(34.0),
                                    height: Val::Px(34.0),
                                    margin: UiRect::right(Val::Px(8.0)),
                                    ..default()
                                },
                                ImageNode {
                                    color: Color::NONE,
                                    ..default()
                                },
                                SpellIcon(spell),
                                Pickable::IGNORE,
                            ));
                            b.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },
                                Pickable::IGNORE,
                            ))
                            .with_children(|tx| {
                                tx.spawn(text(
                                    f,
                                    name,
                                    15.0,
                                    if open { Color::WHITE } else { DIM },
                                ));
                                tx.spawn(text(
                                    f,
                                    format!("{have}/{max}"),
                                    13.0,
                                    if addable { GREEN } else { DIM },
                                ));
                            });
                        });
                    }
                });
            }
        });
        ui.spawn(Node {
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(button_bundle(TalentButton::Reset, 180.0, 44.0))
                .with_child(text(f, "Reset", 18.0, Color::WHITE));
            row.spawn(button_bundle(TalentButton::Close, 180.0, 44.0))
                .with_child(text(f, "Back", 18.0, Color::WHITE));
        });
    });
}
