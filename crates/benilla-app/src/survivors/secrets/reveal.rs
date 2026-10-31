//! The unlock moment (`docs/superpowers/specs/2026-10-31-secret-races-design.md`): each
//! [`Revealed`] becomes a gold banner at the top centre, "A secret has been revealed…" over the
//! race's name, a large-tier event in the `game-feel` sense:
//!
//! - **Motion:** it pops in, scale 0.6 → ~1.05 → 1.0 over [`POP`] s (ease-out-back), holds
//!   [`HOLD`] s and fades over [`FADE`] s (ease-in), back to nothing. A soft gold glow behind it
//!   fades with it.
//! - **Sound:** the quest-complete fanfare, `SoundEntries` `QUESTCOMPLETED` ([`FANFARE`]), the
//!   one 1.12 plays for a finished deed; `LEVELUPSOUND` would read as a level-up, which runs hear
//!   every few seconds.
//! - **Shake:** one large-tier shake (`super::super::feedback`), scaled by the Shake setting, none
//!   when it is Off.
//! - **Queue:** several reveals play one after another, never overlapping.
//! - **Waits:** nothing shows over the card pick or the pause menu ([`can_show`]); a reveal that
//!   arrives there waits for play or the results, and one already showing freezes, hidden.
//!
//! It sits over the boss frames' band (`super::super::bossbar`) and takes precedence there while
//! it shows, ~3 s; on the results it rides higher, over the title. The results also list every
//! race the run revealed ([`RevealedLine`]).

use std::collections::VecDeque;

use bevy::prelude::*;
use benilla_world::view::WorldCamera;

use super::{def, Revealed, SecretRace};
use crate::camera_shake::CameraShakes;
use crate::survivors::feedback::{self, ShakeSetting, Tier};
use crate::survivors::hud::HudFont;
use crate::survivors::Phase;

/// The pop: 0.6 → overshoot → 1.0.
pub(super) const POP: f32 = 0.35;
pub(super) const HOLD: f32 = 2.5;
pub(super) const FADE: f32 = 0.4;
pub(super) const TOTAL: f32 = POP + HOLD + FADE;
/// The banner's first frame scale.
const START_SCALE: f32 = 0.6;
/// Ease-out-back's overshoot constant: 2.0 peaks at ~1.053 over 0.6 → 1.0.
const BACK: f32 = 2.0;
/// The banner's alpha rises over this, inside the pop.
const FADE_IN: f32 = 0.12;
/// `SoundEntries.dbc` `QUESTCOMPLETED`, by name as the client plays it on a turn-in.
pub(super) const FANFARE: &str = "QUESTCOMPLETED";

const GOLD: Color = Color::srgb(1.0, 0.82, 0.0);
const PALE_GOLD: Color = Color::srgb(1.0, 0.93, 0.7);
/// Under the clock line, over the boss frames' first rows.
const TOP_PLAYING: f32 = 84.0;
/// On the results, above the title.
const TOP_RESULTS: f32 = 16.0;

/// The banner's scale `t` seconds in: ease-out-back from 0.6 to 1.0 over [`POP`], then 1.0.
pub(super) fn banner_scale(t: f32) -> f32 {
    if t >= POP {
        return 1.0;
    }
    let x = (t / POP).clamp(0.0, 1.0) - 1.0;
    let back = 1.0 + (BACK + 1.0) * x * x * x + BACK * x * x;
    START_SCALE + (1.0 - START_SCALE) * back
}

/// The banner's (and its glow's) opacity `t` seconds in: a quick rise, the hold, an ease-in fade.
pub(super) fn banner_alpha(t: f32) -> f32 {
    if t < FADE_IN {
        (t / FADE_IN).max(0.0)
    } else if t < POP + HOLD {
        1.0
    } else {
        let x = ((t - POP - HOLD) / FADE).clamp(0.0, 1.0);
        1.0 - x * x
    }
}

/// Whether a banner may show in `phase`: in play, on the results and at the menu, never over the
/// card pick, the pause menu or a loading screen.
pub(super) fn can_show(phase: Phase, loading: bool) -> bool {
    !loading && matches!(phase, Phase::Playing | Phase::GameOver | Phase::Menu)
}

/// The reveals waiting, the one showing and how far in, and the run's reveals for the results.
#[derive(Resource, Default, Debug)]
pub(super) struct Reveals {
    pending: VecDeque<SecretRace>,
    showing: Option<(SecretRace, f32)>,
    /// Every race this run revealed, for the results line.
    this_run: Vec<SecretRace>,
}

impl Reveals {
    pub fn push(&mut self, r: SecretRace) {
        self.pending.push_back(r);
        if !self.this_run.contains(&r) {
            self.this_run.push(r);
        }
    }

    /// Advance `dt` seconds. While it may not show, everything waits, the banner showing too.
    /// Returns the race whose banner starts this tick, after the previous one has ended.
    pub fn tick(&mut self, dt: f32, can_show: bool) -> Option<SecretRace> {
        if !can_show {
            return None;
        }
        if let Some((_, t)) = &mut self.showing {
            *t += dt;
            if *t < TOTAL {
                return None;
            }
            self.showing = None;
        }
        let r = self.pending.pop_front()?;
        self.showing = Some((r, 0.0));
        Some(r)
    }

    pub fn showing(&self) -> Option<(SecretRace, f32)> {
        self.showing
    }
}

/// The results' line of this run's reveals.
#[derive(Component)]
pub(in crate::survivors) struct RevealedLine;

/// The results line's text: empty when the run revealed nothing.
pub(super) fn results_line(races: &[SecretRace]) -> String {
    let names: Vec<&str> = races.iter().map(|&r| def(r).name).collect();
    match names.len() {
        0 => String::new(),
        1 => format!("Secret revealed: {}", names[0]),
        _ => format!("Secrets revealed: {}", names.join(", ")),
    }
}

#[derive(Component)]
struct BannerRoot;
#[derive(Component)]
struct Banner;
#[derive(Component)]
struct BannerName;
/// A banner text's colour at full opacity.
#[derive(Component)]
struct BaseColor(Color);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Reveals>()
        .add_systems(Startup, spawn_banner.after(crate::survivors::hud::setup_hud))
        .add_systems(OnExit(Phase::Menu), new_run)
        .add_systems(
            Update,
            (queue, play, draw, results)
                .chain()
                .after(super::count)
                .before(crate::camera_shake::apply_camera_shake),
        );
}

fn spawn_banner(mut commands: Commands, font: Option<Res<HudFont>>) {
    let Some(font) = font else { return };
    let f = &font.0;
    let text = |s: &str, size: f32, color: Color| {
        (
            Text::new(s),
            TextFont {
                font: f.clone(),
                font_size: size,
                ..default()
            },
            TextColor(color),
            BaseColor(color),
            TextShadow {
                offset: Vec2::splat(2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.9),
            },
            TextLayout::new_with_justify(Justify::Center),
            Pickable::IGNORE,
        )
    };
    commands
        .spawn((
            BannerRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(TOP_PLAYING),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            // Over the HUD and the results (440), under the tooltip.
            GlobalZIndex(450),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Banner,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(2.0),
                    padding: UiRect::axes(Val::Px(120.0), Val::Px(12.0)),
                    ..default()
                },
                glow(0.0),
                UiTransform::IDENTITY,
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                b.spawn(text("A secret has been revealed…", 22.0, PALE_GOLD));
                b.spawn((text("", 40.0, GOLD), BannerName));
            });
        });
}

/// The soft gold glow behind the banner: dark at the centre for the text, gold-edged, gone at
/// both ends, all at `alpha`.
fn glow(alpha: f32) -> BackgroundGradient {
    let c = |r: f32, g: f32, b: f32, a: f32| Color::srgba(r, g, b, a * alpha);
    BackgroundGradient::from(LinearGradient::to_right(vec![
        ColorStop::percent(c(0.5, 0.35, 0.05, 0.0), 0.0),
        ColorStop::percent(c(0.45, 0.3, 0.04, 0.55), 22.0),
        ColorStop::percent(c(0.12, 0.07, 0.01, 0.85), 50.0),
        ColorStop::percent(c(0.45, 0.3, 0.04, 0.55), 78.0),
        ColorStop::percent(c(0.5, 0.35, 0.05, 0.0), 100.0),
    ]))
}

fn new_run(mut reveals: ResMut<Reveals>, mut line: Query<&mut Text, With<RevealedLine>>) {
    reveals.this_run.clear();
    for mut t in &mut line {
        if !t.0.is_empty() {
            t.0.clear();
        }
    }
}

fn queue(mut revealed: MessageReader<Revealed>, mut reveals: ResMut<Reveals>) {
    for r in revealed.read() {
        reveals.push(r.0);
    }
}

/// Advance the queue in real time (play may be paused); a banner that starts sounds and shakes.
#[allow(clippy::too_many_arguments)]
fn play(
    real: Res<Time<Real>>,
    time: Res<Time>,
    phase: Res<State<Phase>>,
    loading: Option<Res<crate::loading_screen::LoadingScreen>>,
    mut reveals: ResMut<Reveals>,
    mut sounds: MessageWriter<crate::sound::GlueSound>,
    camera: Query<&Transform, With<WorldCamera>>,
    mut shakes: ResMut<CameraShakes>,
) {
    if reveals.showing.is_none() && reveals.pending.is_empty() {
        return;
    }
    let phase = *phase.get();
    let ok = can_show(phase, loading.is_some_and(|l| l.covering()));
    let Some(r) = reveals.tick(real.delta_secs(), ok) else {
        return;
    };
    debug!("survivors: revealing {}", def(r).name);
    sounds.write(crate::sound::GlueSound(FANFARE));
    // The view shakes only where there is a run to shake: in play and on its results.
    if !matches!(phase, Phase::Playing | Phase::GameOver) {
        return;
    }
    let scale = feedback::scaled(Tier::Large, ShakeSetting::current());
    let Ok(eye) = camera.single().map(|t| t.translation) else {
        return;
    };
    if scale > 0.0 {
        let now = time.elapsed_secs();
        for row in feedback::rows(Tier::Large, scale) {
            shakes.add(row, eye, now);
        }
    }
}

#[allow(clippy::type_complexity)]
fn draw(
    reveals: Res<Reveals>,
    phase: Res<State<Phase>>,
    loading: Option<Res<crate::loading_screen::LoadingScreen>>,
    mut root: Query<(&mut Node, &mut Visibility), With<BannerRoot>>,
    mut banner: Query<(&mut UiTransform, &mut BackgroundGradient), With<Banner>>,
    mut name: Query<&mut Text, With<BannerName>>,
    mut texts: Query<(&BaseColor, &mut TextColor)>,
    mut named: Local<Option<SecretRace>>,
    mut drawn: Local<Option<(f32, f32)>>,
) {
    let showing = reveals.showing();
    let ok = can_show(*phase.get(), loading.is_some_and(|l| l.covering()));
    let Ok((mut node, mut vis)) = root.single_mut() else {
        return;
    };
    let want = if showing.is_some() && ok {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *vis != want {
        *vis = want;
    }
    let Some((race, t)) = showing.filter(|_| ok) else {
        if showing.is_none() {
            *named = None;
        }
        return;
    };
    // A queued banner follows straight on: name it on its first frame.
    if *named != Some(race) {
        *named = Some(race);
        if let Ok(mut n) = name.single_mut() {
            n.0 = def(race).name.to_string();
        }
    }
    let top = Val::Px(if *phase.get() == Phase::GameOver {
        TOP_RESULTS
    } else {
        TOP_PLAYING
    });
    if node.top != top {
        node.top = top;
    }
    let (scale, alpha) = (banner_scale(t), banner_alpha(t));
    // The hold is ~2.5 s of the same frame: rebuild the gradient and recolour only on a change.
    if *drawn == Some((scale, alpha)) {
        return;
    }
    *drawn = Some((scale, alpha));
    if let Ok((mut tf, mut bg)) = banner.single_mut() {
        tf.scale = Vec2::splat(scale);
        *bg = glow(alpha);
    }
    for (base, mut color) in &mut texts {
        color.0 = base.0.with_alpha(base.0.alpha() * alpha);
    }
}

/// The results list the run's reveals under the score.
fn results(
    reveals: Res<Reveals>,
    phase: Res<State<Phase>>,
    mut line: Query<&mut Text, With<RevealedLine>>,
) {
    // On entering the results (a race revealed mid-run is already in `this_run`) and on every
    // reveal while they show (the run's end can unlock one).
    if *phase.get() != Phase::GameOver || !(reveals.is_changed() || phase.is_changed()) {
        return;
    }
    let want = results_line(&reveals.this_run);
    for mut t in &mut line {
        if t.0 != want {
            t.0 = want.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_pops_overshoots_a_little_and_settles() {
        assert!((banner_scale(0.0) - 0.6).abs() < 1e-5);
        assert!((banner_scale(POP) - 1.0).abs() < 1e-5);
        assert!((banner_scale(2.0) - 1.0).abs() < 1e-5);
        let peak = (0..=350)
            .map(|ms| banner_scale(ms as f32 / 1000.0))
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.03 && peak <= 1.06, "peak {peak}");
        // Eased, not linear: past halfway in scale well before halfway in time.
        assert!(banner_scale(POP * 0.3) > 0.8);
    }

    #[test]
    fn the_banner_fades_in_holds_and_fades_out_to_nothing() {
        assert_eq!(banner_alpha(0.0), 0.0);
        assert_eq!(banner_alpha(POP), 1.0);
        assert_eq!(banner_alpha(POP + HOLD), 1.0);
        // Ease-in: the fade starts slow.
        assert!(banner_alpha(POP + HOLD + FADE * 0.5) > 0.7);
        assert_eq!(banner_alpha(TOTAL), 0.0);
        assert!((TOTAL - 3.25).abs() < 1e-5);
    }

    #[test]
    fn two_reveals_show_in_order_one_after_the_other() {
        let mut q = Reveals::default();
        q.push(SecretRace::Dragonspawn);
        q.push(SecretRace::Felguard);
        assert_eq!(q.tick(0.016, true), Some(SecretRace::Dragonspawn));
        // The second waits through the whole first banner.
        let mut t = 0.0;
        while t + 0.1 < TOTAL {
            assert_eq!(q.tick(0.1, true), None);
            t += 0.1;
            assert_eq!(q.showing().map(|s| s.0), Some(SecretRace::Dragonspawn));
        }
        let mut started = None;
        for _ in 0..3 {
            if let Some(r) = q.tick(0.1, true) {
                started = Some(r);
                break;
            }
        }
        assert_eq!(started, Some(SecretRace::Felguard));
        assert_eq!(q.showing(), Some((SecretRace::Felguard, 0.0)));
        // Both are listed for the results, once each.
        q.push(SecretRace::Felguard);
        assert_eq!(
            results_line(&q.this_run),
            "Secrets revealed: Dragonspawn, Felguard"
        );
    }

    #[test]
    fn a_reveal_during_the_card_pick_waits_for_play() {
        assert!(!can_show(Phase::LevelUp, false));
        assert!(!can_show(Phase::Paused, false));
        assert!(can_show(Phase::Playing, false));
        assert!(can_show(Phase::GameOver, false));
        assert!(!can_show(Phase::Playing, true));
        let mut q = Reveals::default();
        q.push(SecretRace::Kobold);
        for _ in 0..100 {
            assert_eq!(q.tick(0.1, can_show(Phase::LevelUp, false)), None);
        }
        assert_eq!(q.showing(), None);
        assert_eq!(q.tick(0.016, can_show(Phase::Playing, false)), Some(SecretRace::Kobold));
        // A card pick mid-banner freezes it where it was.
        q.tick(1.0, true);
        q.tick(5.0, false);
        assert!((q.showing().unwrap().1 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_results_line_names_the_run_s_reveals() {
        assert_eq!(results_line(&[]), "");
        assert_eq!(results_line(&[SecretRace::Murloc]), "Secret revealed: Murloc");
    }
}
