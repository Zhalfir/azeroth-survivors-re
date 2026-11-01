//! The level-up choices, as a compact stack over the action thumb's cluster, and the rarity each
//! wears in WoW's item-quality colours, so what is better reads at a glance: a fresh spell by its
//! power among the class's, a rank up Uncommon, a rank that adds a target Epic, the top rank
//! Legendary. The run keeps going while a set waits (see the stack's notes below).

use bevy::prelude::*;

use super::data::{self, Kind};
use super::hud::{self, Choice, HudFont, Offer, SpellIcon, SAFE_BOTTOM};
use super::settings::StickSide;
use super::{Phase, Rng, Run};

/// WoW's item-quality colours (`ITEM_QUALITY_COLORS`).
pub(super) const COMMON: Color = Color::srgb(1.0, 1.0, 1.0);
pub(super) const UNCOMMON: Color = Color::srgb(0.118, 1.0, 0.0);
pub(super) const RARE: Color = Color::srgb(0.0, 0.439, 0.867);
pub(super) const EPIC: Color = Color::srgb(0.639, 0.208, 0.933);
pub(super) const LEGENDARY: Color = Color::srgb(1.0, 0.502, 0.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub(super) fn color(self) -> Color {
        match self {
            Self::Common => COMMON,
            Self::Uncommon => UNCOMMON,
            Self::Rare => RARE,
            Self::Epic => EPIC,
            Self::Legendary => LEGENDARY,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Uncommon => "Uncommon",
            Self::Rare => "Rare",
            Self::Epic => "Epic",
            Self::Legendary => "Legendary",
        }
    }
}

/// A spell's rough damage per second at rank 1 against a crowd: per-cast damage times the
/// enemies it can reach, over its cooldown. Only for ranking a class's own spells against each
/// other; the defensive kinds have none and sit in the middle.
fn power(def: &data::AbilityDef) -> Option<f32> {
    let area = |r: f32| 1.0 + r * 0.6;
    let reach = match def.kind {
        Kind::Bolt | Kind::Strike => def.count as f32 + def.radius * 0.4,
        Kind::Melee | Kind::Orbit => def.count as f32,
        Kind::Nova | Kind::Cone | Kind::Ground | Kind::GroundSelf | Kind::Aura => area(def.radius),
        Kind::Chain => def.count as f32 + 1.0,
        Kind::Dot => def.count as f32,
        _ => return None,
    };
    let per = if def.damage > 0.0 {
        def.damage
    } else {
        def.dot * def.duration.max(1.0)
    };
    let ticks = if matches!(def.kind, Kind::Ground | Kind::GroundSelf) && def.duration > 0.0 {
        def.duration
    } else {
        1.0
    };
    Some(per * ticks * reach / def.cooldown.max(0.2))
}

/// Where a new spell stands among its class's: the strongest third Epic, the weakest third
/// Uncommon, the rest (and the defensive spells) Rare.
fn spell_tier(def: &data::AbilityDef, run: &Run) -> Rarity {
    let Some(mine) = power(def) else {
        return Rarity::Rare;
    };
    let mut all: Vec<f32> = run
        .class
        .pool
        .iter()
        .filter_map(|k| power(data::ability(k)))
        .collect();
    all.sort_by(f32::total_cmp);
    let rank = all.iter().filter(|&&p| p < mine).count() as f32 / all.len().max(1) as f32;
    if rank >= 0.66 {
        Rarity::Epic
    } else if rank < 0.33 {
        Rarity::Uncommon
    } else {
        Rarity::Rare
    }
}

/// What a choice is worth, by what it brings: a top rank is Legendary, a rank that adds a target
/// Epic, a new spell by its power among the class's ([`spell_tier`]), a plain rank Uncommon.
pub(super) fn rarity(choice: Choice, run: &Run) -> Rarity {
    match choice {
        Choice::NewAbility(def) => spell_tier(def, run),
        Choice::Upgrade(k) => {
            let o = &run.abilities[k];
            let next = o.level + 1;
            let targets = [3, 5, 7].contains(&next)
                && matches!(
                    o.def.kind,
                    Kind::Bolt | Kind::Strike | Kind::Melee | Kind::Dot | Kind::Chain | Kind::Orbit
                );
            if next == data::MAX_ABILITY_LEVEL {
                Rarity::Legendary
            } else if targets {
                Rarity::Epic
            } else {
                Rarity::Uncommon
            }
        }
        Choice::NewPassive(_) => Rarity::Uncommon,
        Choice::UpPassive(k) => match run.passives[k].1 + 1 {
            n if n == data::MAX_PASSIVE_LEVEL => Rarity::Epic,
            n if n + 1 == data::MAX_PASSIVE_LEVEL => Rarity::Rare,
            2 => Rarity::Common,
            _ => Rarity::Uncommon,
        },
        Choice::Heal => Rarity::Common,
    }
}

// --- The level-up stack. ---
//
// The choices wait in a compact stack over the action thumb's cluster while the run goes on at
// full speed (the user's choice, 2026-11-01, `docs/superpowers/mockups/thumb-hud.png`). The run
// never leaves `Phase::Playing` for a level: `Run::pending_levels` counts the sets owed, [`Offer`]
// holds the one on show, and [`Stack`] the row picked out. A tap (or 1, 2, 3) picks a row out
// and opens its tooltip toward the screen's centre; the same row again (or Enter, or the tooltip)
// takes it; a tap anywhere else, Escape, or [`SELECT_SECS`] untouched lets it go.

/// The stack's width and a row's height, in UI units: a thumb's target (at least 44).
pub(super) const STACK_W: f32 = 220.0;
pub(super) const ROW_H: f32 = 48.0;
const ROW_GAP: f32 = 6.0;
const HEADER_H: f32 = 28.0;
/// The stack's inset from the action side's edge.
pub(super) const STACK_SIDE: f32 = 12.0;
/// The stack's foot: over the action arc (F, Q, C and the R slot kept for Phase C, `touch`)
/// and its key hints.
pub(super) const STACK_BOTTOM: f32 = SAFE_BOTTOM + 244.0;
/// The tooltip's width and its gap from the stack.
const TIP_W: f32 = 250.0;
const TIP_GAP: f32 = 10.0;
/// A picked-out row lets go after this long untouched (real seconds).
pub(super) const SELECT_SECS: f32 = 4.0;
/// A new set slides in from the side edge over this long, from this far out.
const SLIDE_SECS: f32 = 0.15;
const SLIDE_FROM: f32 = 60.0;

const ROW_BG: Color = Color::srgba(0.07, 0.065, 0.055, 0.92);
const ROW_BG_HOT: Color = Color::srgba(0.25, 0.17, 0.05, 0.95);
const BADGE: Color = Color::srgb(0.73, 0.46, 0.09);
const BADGE_TEXT: Color = Color::srgb(0.25, 0.14, 0.01);
const TAKE_GREEN: Color = Color::srgb(0.59, 0.77, 0.35);

#[cfg(not(target_os = "android"))]
const TAKE_HINT: &str = "Click again, or Enter, to take";
#[cfg(target_os = "android")]
const TAKE_HINT: &str = "Tap again to take";

/// One input to the stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StackInput {
    /// A row tapped or clicked, or its key (1, 2, 3).
    Press(usize),
    /// Enter, or the tooltip tapped: take the row picked out.
    Confirm,
    /// A tap or click anywhere else.
    Away,
    /// Escape.
    Cancel,
}

/// The row picked out, and the clocks of the stack's motion.
#[derive(Resource, Default, Debug)]
pub(super) struct Stack {
    pub selected: Option<usize>,
    /// Real seconds since the selection was last touched.
    idle: f32,
    /// Real seconds since the set on show was dealt, for its slide-in.
    slide: f32,
    /// The [`Offer`] serial the rows were built for.
    built: Option<u32>,
}

impl Stack {
    /// One input over `len` rows; the row to take, if this input takes one.
    pub(super) fn input(&mut self, input: StackInput, len: usize) -> Option<usize> {
        match input {
            StackInput::Press(i) if i >= len => None,
            StackInput::Press(i) if self.selected == Some(i) => {
                self.selected = None;
                Some(i)
            }
            StackInput::Press(i) => {
                self.selected = Some(i);
                self.idle = 0.0;
                None
            }
            StackInput::Confirm => self.selected.take().filter(|&i| i < len),
            StackInput::Away | StackInput::Cancel => {
                self.selected = None;
                None
            }
        }
    }

    /// `dt` real seconds on: a selection left alone for [`SELECT_SECS`] lets go.
    pub(super) fn tick(&mut self, dt: f32) {
        self.slide += dt;
        if self.selected.is_some() {
            self.idle += dt;
            if self.idle >= SELECT_SECS {
                self.selected = None;
            }
        }
    }
}

/// The header's badge: "+N" while more than one set is owed.
pub(super) fn pending_badge(pending: u32) -> Option<String> {
    (pending > 1).then(|| format!("+{pending}"))
}

/// Whether the stack shows: a set is owed and dealt, and the hero stands.
pub(super) fn stack_visible(run: &Run, offer: &Offer) -> bool {
    run.pending_levels > 0 && offer.len() > 0 && run.dead_t.is_none()
}

/// Taps the touch layer saw this frame that landed off the stack (`touch`, Android).
#[derive(Resource, Default)]
pub(super) struct StackTaps {
    pub away: bool,
}

#[derive(Component)]
pub(super) struct StackRoot;
#[derive(Component)]
pub(super) struct StackTitle;
#[derive(Component)]
pub(super) struct StackBadge;
#[derive(Component)]
pub(super) struct StackBadgeText;
#[derive(Component)]
pub(super) struct StackRows;
/// A row of the stack and the choice it shows.
#[derive(Component)]
pub(super) struct StackRow {
    pub index: usize,
    rarity: Rarity,
}
/// The selected row's tooltip; a tap on it takes the row.
#[derive(Component)]
pub(super) struct StackTip;
#[derive(Component)]
pub(super) struct StackTipName;
#[derive(Component)]
pub(super) struct StackTipBody;

/// The stack, its header and its tooltip; the rows come with each set ([`build_stack`]).
pub(super) fn spawn_stack(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
    commands
        .spawn((
            StackRoot,
            hud::ThumbAnchor(STACK_SIDE),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(STACK_BOTTOM),
                right: Val::Px(STACK_SIDE),
                width: Val::Px(STACK_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ROW_GAP),
                display: Display::None,
                ..default()
            },
            GlobalZIndex(405),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn(Node {
                height: Val::Px(HEADER_H),
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|h| {
                h.spawn((
                    StackTitle,
                    hud::text_bundle(f, "Level up", 20.0, hud::GOLD_TONE),
                ));
                h.spawn((
                    StackBadge,
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)),
                        border_radius: BorderRadius::MAX,
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(BADGE),
                ))
                .with_children(|b| {
                    b.spawn((
                        StackBadgeText,
                        Text::new(""),
                        TextFont {
                            font: f.clone(),
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(BADGE_TEXT),
                    ));
                });
            });
            root.spawn((
                StackRows,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(ROW_GAP),
                    ..default()
                },
            ));
            // The tooltip, beside the selected row toward the screen's centre ([`animate_stack`]).
            root.spawn((
                StackTip,
                Button,
                Interaction::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(STACK_W + TIP_GAP),
                    top: Val::Px(0.0),
                    width: Val::Px(TIP_W),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.07, 0.065, 0.055, 0.95)),
                BorderColor::all(hud::GOLD_TONE),
            ))
            .with_children(|t| {
                t.spawn((
                    StackTipName,
                    hud::text_bundle(f, "", 18.0, hud::GOLD_TONE),
                ));
                t.spawn((
                    StackTipBody,
                    hud::text_bundle(f, "", 15.0, Color::srgb(0.86, 0.85, 0.8)),
                ));
                t.spawn(hud::text_bundle(f, TAKE_HINT, 14.0, TAKE_GREEN));
            });
        });
}

/// Keeps a set dealt while levels are owed, and none once they are not: a level from a gem, a
/// chest, or the next set after a take shows at once; a maxed build's banked heals clear it.
pub(super) fn deal_offer(run: Res<Run>, mut rng: ResMut<Rng>, mut offer: ResMut<Offer>) {
    if run.pending_levels == 0 {
        if offer.len() > 0 {
            offer.clear();
        }
    } else if offer.len() == 0 {
        hud::deal(&run, &mut rng, &mut offer);
    }
}

/// Taps, clicks and keys on the stack, and the selection's timeout. Only in play: the pause
/// menu, the results and the menu leave it be.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn stack_input(
    time: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    rows: Query<(&StackRow, &Interaction), Changed<Interaction>>,
    targets: Query<&Interaction, Or<(With<StackRow>, With<StackTip>)>>,
    tip: Query<&Interaction, (With<StackTip>, Changed<Interaction>)>,
    mut taps: ResMut<StackTaps>,
    mut stack: ResMut<Stack>,
    mut run: ResMut<Run>,
    mut offer: ResMut<Offer>,
    mut rng: ResMut<Rng>,
) {
    let away = std::mem::take(&mut taps.away);
    if *phase.get() != Phase::Playing || !stack_visible(&run, &offer) {
        if stack.selected.is_some() {
            stack.selected = None;
        }
        return;
    }
    stack.tick(time.delta_secs());
    let mut inputs: Vec<StackInput> = Vec::new();
    for (row, i) in &rows {
        if *i == Interaction::Pressed {
            inputs.push(StackInput::Press(row.index));
        }
    }
    if tip.iter().any(|i| *i == Interaction::Pressed) {
        inputs.push(StackInput::Confirm);
    }
    for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .into_iter()
        .enumerate()
    {
        if keys.just_pressed(key) {
            inputs.push(StackInput::Press(i));
        }
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        inputs.push(StackInput::Confirm);
    }
    if keys.just_pressed(KeyCode::Escape) {
        inputs.push(StackInput::Cancel);
    }
    // A click off the stack (desktop), or a finger that landed off it (`touch`).
    let on_stack = targets.iter().any(|i| *i == Interaction::Pressed);
    if away || (mouse.just_pressed(MouseButton::Left) && !on_stack) {
        inputs.push(StackInput::Away);
    }
    for input in inputs {
        if let Some(i) = stack.input(input, offer.len()) {
            take(i, &mut stack, &mut run, &mut offer, &mut rng);
            break;
        }
    }
}

/// Take row `i`: the choice applies and the next set, if any, slides in.
pub(super) fn take(i: usize, stack: &mut Stack, run: &mut Run, offer: &mut Offer, rng: &mut Rng) {
    stack.selected = None;
    hud::pick(i, run, offer, rng);
}

/// The bot's press, through the same reducer a tap goes through (`auto`).
pub(super) fn bot_press(i: usize, stack: &mut Stack, run: &mut Run, offer: &mut Offer, rng: &mut Rng) {
    if let Some(i) = stack.input(StackInput::Press(i), offer.len()) {
        take(i, stack, run, offer, rng);
    }
}

/// The rows, rebuilt for each set dealt; the badge, on a change of the count owed.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn build_stack(
    mut commands: Commands,
    offer: Res<Offer>,
    run: Res<Run>,
    side: Res<StickSide>,
    spells: Option<Res<crate::ui_action::Spells>>,
    font: Option<Res<HudFont>>,
    mut stack: ResMut<Stack>,
    rows: Query<Entity, With<StackRows>>,
    mut badge: Query<&mut Node, With<StackBadge>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<StackBadgeText>>,
        Query<(&mut Text, &mut TextColor), With<StackTitle>>,
    )>,
    mut owed: Local<Option<(u32, bool)>>,
) {
    let Some(font) = font else { return };
    let f = &font.0;
    let key = (run.pending_levels, offer.is_chest());
    if *owed != Some(key) {
        *owed = Some(key);
        let label = pending_badge(run.pending_levels);
        if let Ok(mut node) = badge.single_mut() {
            let display = if label.is_some() {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
        if let Ok(mut t) = texts.p0().single_mut() {
            let s = label.unwrap_or_default();
            if t.0 != s {
                t.0 = s;
            }
        }
        // A boss's chest offers through the same stack, under its own gold header.
        let title = if offer.is_chest() { "Chest" } else { "Level up" };
        if let Ok((mut t, mut c)) = texts.p1().single_mut() {
            if t.0 != title {
                t.0 = title.to_string();
            }
            let color = if offer.is_chest() {
                LEGENDARY
            } else {
                hud::GOLD_TONE
            };
            if c.0 != color {
                c.0 = color;
            }
        }
    }
    if stack.built == Some(offer.serial()) {
        return;
    }
    stack.built = Some(offer.serial());
    stack.selected = None;
    stack.slide = 0.0;
    let Ok(row_box) = rows.single() else { return };
    commands.entity(row_box).despawn_related::<Children>();
    // Out at the side edge until the slide-in moves it ([`animate_stack`]).
    let out = match *side {
        StickSide::Left => SLIDE_FROM,
        StickSide::Right => -SLIDE_FROM,
    };
    commands.entity(row_box).with_children(|list| {
        for (i, &choice) in offer.choices().iter().enumerate() {
            let card = hud::describe_card(choice, &run, spells.as_deref());
            let rarity = rarity(choice, &run);
            let rc = rarity.color();
            list.spawn((
                StackRow { index: i, rarity },
                Button,
                Interaction::default(),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(ROW_H),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(ROW_BG),
                BorderColor::all(rc.mix(&Color::BLACK, 0.45)),
                UiTransform {
                    translation: Val2::px(out, 0.0),
                    ..default()
                },
            ))
            .with_children(|r| {
                r.spawn((
                    Node {
                        width: Val::Px(36.0),
                        height: Val::Px(36.0),
                        flex_shrink: 0.0,
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    ImageNode {
                        color: Color::srgba(0.0, 0.0, 0.0, 0.6),
                        ..default()
                    },
                    SpellIcon(card.spell),
                ));
                r.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    overflow: Overflow::clip_x(),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        hud::text_bundle(f, card.name, 16.0, rc.mix(&Color::WHITE, 0.6)),
                        TextLayout::new(Justify::Left, LineBreak::NoWrap),
                    ));
                    col.spawn((
                        hud::text_bundle(
                            f,
                            format!("{} · {}", card.tag, rarity.label()),
                            hud::TEXT_FLOOR,
                            rc.mix(&Color::WHITE, 0.25),
                        ),
                        TextLayout::new(Justify::Left, LineBreak::NoWrap),
                    ));
                });
                // The key, where there is a keyboard.
                #[cfg(not(target_os = "android"))]
                r.spawn(hud::text_bundle(
                    f,
                    format!("{}", i + 1),
                    hud::TEXT_FLOOR,
                    hud::DIM_GREY,
                ));
            });
        }
    });
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Shows the stack while a set waits in play, slides a new set in, lights the selected row and
/// seats its tooltip toward the screen's centre. Every write only on a change.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn animate_stack(
    phase: Res<State<Phase>>,
    run: Res<Run>,
    offer: Res<Offer>,
    stack: Res<Stack>,
    side: Res<StickSide>,
    spells: Option<Res<crate::ui_action::Spells>>,
    mut root: Query<(&mut Node, &mut Visibility), (With<StackRoot>, Without<StackTip>)>,
    mut rows: Query<
        (
            &StackRow,
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut UiTransform,
        ),
        Without<StackTip>,
    >,
    mut tip: Query<(&mut Node, &mut Visibility), (With<StackTip>, Without<StackRoot>)>,
    mut tip_texts: ParamSet<(
        Query<&mut Text, With<StackTipName>>,
        Query<&mut Text, With<StackTipBody>>,
    )>,
    mut shown: Local<Option<(u32, usize)>>,
) {
    let on = *phase.get() == Phase::Playing && stack_visible(&run, &offer);
    if let Ok((mut node, mut vis)) = root.single_mut() {
        let display = if on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        vis.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !on {
        return;
    }
    let u = ease_out_cubic((stack.slide / SLIDE_SECS).clamp(0.0, 1.0));
    let dir = match *side {
        StickSide::Left => 1.0,
        StickSide::Right => -1.0,
    };
    let x = (dir * SLIDE_FROM * (1.0 - u)).round();
    for (row, interaction, mut bg, mut border, mut tf) in &mut rows {
        let hot = stack.selected == Some(row.index);
        let want_bg = if hot {
            ROW_BG_HOT
        } else if *interaction == Interaction::Hovered {
            ROW_BG.mix(&ROW_BG_HOT, 0.4)
        } else {
            ROW_BG
        };
        if bg.0 != want_bg {
            bg.0 = want_bg;
        }
        let want_edge = if hot {
            BorderColor::all(hud::GOLD_TONE)
        } else {
            BorderColor::all(row.rarity.color().mix(&Color::BLACK, 0.45))
        };
        if *border != want_edge {
            *border = want_edge;
        }
        let at = Val2::px(x, 0.0);
        if tf.translation != at {
            tf.translation = at;
        }
    }
    let Ok((mut node, mut vis)) = tip.single_mut() else {
        return;
    };
    let Some(i) = stack.selected.filter(|&i| i < offer.len()) else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        vis.set_if_neq(Visibility::Hidden);
        *shown = None;
        return;
    };
    // Beside the row, toward the centre: left of a right-hand stack, right of a left-hand one.
    let (left, right) = match *side {
        StickSide::Left => (Val::Auto, Val::Px(STACK_W + TIP_GAP)),
        StickSide::Right => (Val::Px(STACK_W + TIP_GAP), Val::Auto),
    };
    let top = Val::Px(HEADER_H + ROW_GAP + i as f32 * (ROW_H + ROW_GAP) - 8.0);
    if node.left != left || node.right != right || node.top != top {
        node.left = left;
        node.right = right;
        node.top = top;
    }
    if node.display != Display::Flex {
        node.display = Display::Flex;
    }
    vis.set_if_neq(Visibility::Inherited);
    if *shown == Some((offer.serial(), i)) {
        return;
    }
    *shown = Some((offer.serial(), i));
    let card = hud::describe_card(offer.choices()[i], &run, spells.as_deref());
    if let Ok(mut t) = tip_texts.p0().single_mut() {
        t.0 = card.name;
    }
    if let Ok(mut t) = tip_texts.p1().single_mut() {
        t.0 = if card.desc.is_empty() {
            card.tag
        } else {
            format!("{}\n{}", card.tag, card.desc)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn stack_shows_while_pending_and_game_runs() {
        let mut world = World::new();
        world.insert_resource(Run {
            pending_levels: 2,
            ..Run::default()
        });
        world.insert_resource(Offer::default());
        world.insert_resource(Rng::default());
        world.insert_resource(Time::<Virtual>::default());
        assert!(!stack_visible(world.resource::<Run>(), world.resource::<Offer>()));
        world.run_system_once(deal_offer).unwrap();
        assert!(stack_visible(world.resource::<Run>(), world.resource::<Offer>()));
        // Dealing a set neither pauses the clock nor leaves play.
        assert!(!world.resource::<Time<Virtual>>().is_paused());

        let mut run = world.remove_resource::<Run>().unwrap();
        let mut offer = world.remove_resource::<Offer>().unwrap();
        let mut rng = world.remove_resource::<Rng>().unwrap();
        let mut stack = Stack::default();
        let serial = offer.serial();
        take(0, &mut stack, &mut run, &mut offer, &mut rng);
        // The next set is dealt at once.
        assert_eq!(run.pending_levels, 1);
        assert!(stack_visible(&run, &offer));
        assert_ne!(offer.serial(), serial);
        take(0, &mut stack, &mut run, &mut offer, &mut rng);
        assert_eq!(run.pending_levels, 0);
        assert!(!stack_visible(&run, &offer));
        // A dead hero's stack hides.
        run.pending_levels = 1;
        hud::deal(&run, &mut rng, &mut offer);
        assert!(stack_visible(&run, &offer));
        run.dead_t = Some(0.0);
        assert!(!stack_visible(&run, &offer));
    }

    #[test]
    fn second_tap_takes_first_tap_selects() {
        let mut s = Stack::default();
        assert_eq!(s.input(StackInput::Press(1), 3), None);
        assert_eq!(s.selected, Some(1));
        // Another row moves the selection; the same row again takes it.
        assert_eq!(s.input(StackInput::Press(2), 3), None);
        assert_eq!(s.selected, Some(2));
        assert_eq!(s.input(StackInput::Press(2), 3), Some(2));
        assert_eq!(s.selected, None);
        // Enter takes the selected row, and nothing without one.
        assert_eq!(s.input(StackInput::Confirm, 3), None);
        s.input(StackInput::Press(0), 3);
        assert_eq!(s.input(StackInput::Confirm, 3), Some(0));
        // A row past the set does nothing.
        assert_eq!(s.input(StackInput::Press(2), 1), None);
        assert_eq!(s.selected, None);
        // A tap elsewhere and Escape let go.
        s.input(StackInput::Press(0), 3);
        assert_eq!(s.input(StackInput::Away, 3), None);
        assert_eq!(s.selected, None);
        s.input(StackInput::Press(0), 3);
        assert_eq!(s.input(StackInput::Cancel, 3), None);
        assert_eq!(s.selected, None);
    }

    #[test]
    fn deselect_after_timeout() {
        let mut s = Stack::default();
        s.input(StackInput::Press(1), 3);
        s.tick(SELECT_SECS - 0.1);
        assert_eq!(s.selected, Some(1));
        // Picking another row restarts the clock.
        s.input(StackInput::Press(0), 3);
        s.tick(SELECT_SECS - 0.1);
        assert_eq!(s.selected, Some(0));
        s.tick(0.2);
        assert_eq!(s.selected, None);
        // With nothing selected the clock is idle: a later pick gets its full time.
        s.tick(10.0);
        s.input(StackInput::Press(2), 3);
        s.tick(1.0);
        assert_eq!(s.selected, Some(2));
    }

    #[test]
    fn pending_badge_counts() {
        assert_eq!(pending_badge(0), None);
        assert_eq!(pending_badge(1), None);
        assert_eq!(pending_badge(2).as_deref(), Some("+2"));
        assert_eq!(pending_badge(7).as_deref(), Some("+7"));
    }
}
