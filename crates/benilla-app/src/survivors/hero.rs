//! The hero: its body, WASD movement over the terrain, the top-down camera, and its health.

use benilla_world::view::WorldCamera;
use benilla_world::world_point::WorldPoint;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use super::data::{self, ClassDef};
use super::units::{self, flat, wow_to_bevy, yaw_toward, Fx, Marks, UnitSpec};
use super::{Phase, Run, Selection};
use crate::creature_anim::{move_flags, MovementState};
use crate::net::{FieldChanged, Guid, ObjectStore};

#[derive(Component)]
pub(super) struct Hero {
    /// Vertical velocity in yards/second while airborne; 0 on the ground.
    pub(super) vy: f32,
    /// Height above the ground while airborne. Kept here, not in the transform: the creature
    /// ground clamp (`net::motion::ground_clamp_creatures`) re-seats every unit on the floor at
    /// the top of the frame, so the arc is re-applied over the ground each frame.
    pub(super) air: f32,
}

/// The body radius enemies stop at.
pub(super) const HERO_RADIUS: f32 = 0.7;

/// The hop's initial upward speed and the gravity that brings it back down — the reference
/// `CMovement` jump seed and gravity (`player/state.rs`'s `JUMP_SPEED` / `GRAVITY`).
const JUMP_SPEED: f32 = 7.955_547;
const GRAVITY: f32 = 19.291_105;

/// Radians of orbit per raw mouse unit while right-dragging.
const ORBIT_RATE: f32 = 0.008;
/// Elevation clamp (radians): ~10°..~83° above the horizon, so the camera never dives underground.
const PITCH_MIN: f32 = 0.18;
const PITCH_MAX: f32 = 1.45;
/// The locked view's tilt.
const FIXED_PITCH: f32 = 61.0 * std::f32::consts::PI / 180.0;
/// Wheel zoom range, yards from the hero's shoulders.
const ZOOM_MIN: f32 = 1.5;
const ZOOM_MAX: f32 = 70.0;

/// Whether the camera is locked; `C` toggles it.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(super) enum CamMode {
    /// Locked at whatever view it was left at: top-down (61°) by default, laptop-friendly — the
    /// original feel.
    #[default]
    Fixed,
    /// Unlocked: a mouse drag swings and tilts it around the hero.
    Orbit,
}

#[derive(Resource)]
pub(super) struct CamRig {
    pub dist: f32,
    pub orbit: f32,
    /// Orbit azimuth (radians) around the hero; right-drag swings it during play.
    pub yaw: f32,
    /// Elevation above the horizon (radians), clamped so the camera never dives underground.
    pub pitch: f32,
    pub mode: CamMode,
}

impl Default for CamRig {
    fn default() -> Self {
        Self {
            dist: 30.0,
            orbit: 0.6,
            yaw: 0.0,
            pitch: FIXED_PITCH,
            mode: CamMode::Fixed,
        }
    }
}

/// Lock or unlock the camera, keeping its view.
fn toggle_lock(rig: &mut CamRig) {
    rig.mode = match rig.mode {
        CamMode::Fixed => CamMode::Orbit,
        CamMode::Orbit => CamMode::Fixed,
    };
}

/// A screen-space move (`x` right, `z` down the screen) turned into the world, for a camera at
/// `yaw`: "up" on the stick or W is always up the screen.
fn move_dir(input: Vec3, yaw: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    Vec3::new(input.x * cy + input.z * sy, 0.0, input.z * cy - input.x * sy)
}

/// How long the hero keeps its heading after a level-up card pick, and the ease-out at its end.
/// On the tablet the finger leaves the stick to tap a card, so the hero used to stop dead after
/// every pick until the thumb found the stick again (user feedback, 2026-10-31). Any input ends
/// the carry at once; the pick's grace second (`super::grace`) covers it.
const CARRY_SECS: f32 = 1.0;
const CARRY_EASE: f32 = 0.2;

/// The heading carried across a card pick.
#[derive(Resource, Default)]
pub(super) struct MoveCarry {
    /// The manual heading on the last playing frame.
    last: Vec3,
    /// The heading carried after the pick.
    dir: Vec3,
    /// Seconds of carry left.
    left: f32,
}

/// Full speed until the last [`CARRY_EASE`] seconds, then a linear ease to a stop.
fn carry_ease(left: f32) -> f32 {
    (left / CARRY_EASE).clamp(0.0, 1.0)
}

/// The level-up cards came up: remember where the hero was heading.
pub(super) fn remember_heading(mut carry: ResMut<MoveCarry>) {
    carry.dir = carry.last;
    carry.left = 0.0;
}

/// The pick is made: carry that heading for [`CARRY_SECS`].
pub(super) fn carry_heading(mut carry: ResMut<MoveCarry>) {
    if carry.dir != Vec3::ZERO {
        carry.left = CARRY_SECS;
    }
}

/// Where a battleground's runs start.
pub(super) fn arena(map: &data::MapDef) -> Vec3 {
    wow_to_bevy(map.center)
}

/// The world is the battleground being stood in ([`Selection::stage`]): entering one on another
/// continent worldports, which raises the loading screen until the new ground is in.
pub(super) fn follow_map(
    selection: Res<Selection>,
    current: Option<ResMut<benilla_world::world_map::CurrentMap>>,
) {
    let Some(mut current) = current else {
        return;
    };
    let want = selection.stage_def().map;
    if current.0 != want {
        current.0 = want;
    }
}

/// The hero in the picked look, weapons drawn. `race` is the look's race slot: a secret race's
/// code turns into its unit race here (`secrets::unit_race`), and `scale` is the whole render
/// scale (1 for the eight races, the secret race's own otherwise).
pub(super) fn spawn_hero(
    commands: &mut Commands,
    class: &'static ClassDef,
    display: u32,
    race: u8,
    sex: u8,
    scale: f32,
    serial: u64,
    pos: Vec3,
) -> Entity {
    let bytes0 = u32::from(super::secrets::unit_race(race))
        | u32::from(class.id) << 8
        | u32::from(sex) << 16
        | u32::from(class.power) << 24;
    let e = units::spawn_unit(
        commands,
        &UnitSpec {
            serial,
            display,
            scale,
            level: 60,
            faction: units::FACTION_FRIENDLY,
            bytes0,
            weapons: class.weapons,
            sheath: class.sheath,
        },
        pos,
        0.0,
    );
    commands
        .entity(e)
        .insert((Hero { vy: 0.0, air: 0.0 }, Marks::buffs()));
    if super::secrets::swings_to_cast(race) {
        commands.entity(e).insert(units::CastsWithSwing);
    }
    e
}

/// Hear from the hero, as the client's `SoundListenerAtCharacter` (default on) hears from the
/// character. The sound plugin seats the listener on the `Embodied` body and otherwise falls back
/// to the camera; the hero has none, so every 3D sound was rolled off from the camera, ~34 yd up
/// at the default zoom: the hero's own `BowRelease` (vol 0.69, min 8 yd) reached the mix at 0.049
/// on the tablet, against music at 0.4 and ambience at 0.6. Zoomed out past a kit's 45 yd cutoff,
/// combat around the hero was not even allocated. Runs between the plugin's seat
/// (`WorldStage::Stream`) and every sound consumer (`WorldStage::Present`).
///
/// The position only: the camera's facing stays, so left and right are the screen's on a top-down
/// view rather than the hero's, which turns with every shot.
pub(super) fn seat_listener(
    config: Res<crate::sound::SoundConfig>,
    hero: Query<(&Transform, Option<&crate::player::CameraPivot>), With<Hero>>,
    mut listener: ResMut<crate::sound::AudioListener>,
    out: Option<NonSendMut<crate::sound::SoundOutput>>,
) {
    if !config.listener_at_character {
        return;
    }
    let Ok((t, pivot)) = hero.single() else {
        return; // no run: the camera's ear
    };
    listener.pos = t.translation + Vec3::Y * crate::player::head_height(pivot, t.scale.x);
    if let Some(mixer) = out.and_then(|o| o.into_inner().mixer.as_mut()) {
        mixer.set_listener(listener.pos, listener.rot);
    }
}

/// Midday over the battleground, whatever the clock says.
pub(super) const MINUTE_OF_DAY: u32 = 780;

pub(super) fn daylight(debug: Option<ResMut<benilla_world::dev_state::DebugState>>) {
    let Some(mut debug) = debug else { return };
    if debug.lighting.follow_server_time || debug.lighting.manual_minute != MINUTE_OF_DAY {
        debug.lighting.follow_server_time = false;
        debug.lighting.manual_minute = MINUTE_OF_DAY;
    }
}

/// The ground under the mouse cursor, `None` with the cursor off the window. The locked camera's
/// hero faces it, and the aimed mobility spells (Blink, Charge) go toward it.
#[derive(Resource, Default)]
pub(super) struct AimPoint(pub(super) Option<Vec3>);

/// Cast the cursor onto the terrain: the camera ray meets the plane at the hero's height, then the
/// terrain height there, a few times over, which settles on hills within a frame.
pub(super) fn aim_cursor(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cams: Query<(&Camera, &Transform), With<WorldCamera>>,
    hero: Query<&Transform, (With<Hero>, Without<WorldCamera>)>,
    world: WorldPoint,
    mut aim: ResMut<AimPoint>,
) {
    let point = (|| {
        let cursor = windows.single().ok()?.cursor_position()?;
        let (cam, tf) = cams.single().ok()?;
        let ray = cam
            .viewport_to_world(&GlobalTransform::from(*tf), cursor)
            .ok()?;
        let mut y = hero.single().ok()?.translation.y;
        let mut at = None;
        for _ in 0..4 {
            if ray.direction.y.abs() < 1e-4 {
                break;
            }
            let t = (y - ray.origin.y) / ray.direction.y;
            if t <= 0.0 {
                break;
            }
            let p = ray.origin + *ray.direction * t;
            at = Some(p);
            match world.terrain_height_under(p) {
                Some(h) if (h - y).abs() > 0.05 => y = h,
                _ => break,
            }
        }
        at
    })();
    if aim.0 != point {
        aim.0 = point;
    }
}

/// WASD over the terrain within the battleground's leash, and the streaming focus on the hero.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_hero(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    run: Res<Run>,
    selection: Res<Selection>,
    rig: Res<CamRig>,
    aim: Res<AimPoint>,
    world: WorldPoint,
    auto: Option<Res<super::auto::Auto>>,
    mut player: ResMut<crate::player::Player>,
    mut carry: ResMut<MoveCarry>,
    mut hero: Query<(&mut Transform, &mut MovementState, &mut Hero)>,
) {
    let focus = match hero.single_mut() {
        Ok((mut tf, mut mv, mut hm)) => {
            let dt = time.delta_secs();
            let mut dir = Vec3::ZERO;
            let mut ease = 1.0;
            if let Some(auto) = auto
                .as_deref()
                .filter(|_| *phase.get() == Phase::Playing && run.dead_t.is_none())
            {
                dir = super::auto::steer(auto);
            } else if *phase.get() == Phase::Playing && run.dead_t.is_none() {
                if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
                    dir.z -= 1.0;
                }
                if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
                    dir.z += 1.0;
                }
                if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
                    dir.x -= 1.0;
                }
                if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
                    dir.x += 1.0;
                }
                // Camera-relative in both modes: a view locked at an angle keeps "up" up the
                // screen. At the default view (yaw 0) this is the old world-relative WASD.
                dir = move_dir(dir, rig.yaw);
                carry.last = dir;
                if dir != Vec3::ZERO {
                    carry.left = 0.0;
                } else if carry.left > 0.0 {
                    // After a card pick the finger is still off the stick: keep going the way the
                    // hero went when the cards came up, easing out (`MoveCarry`).
                    dir = carry.dir;
                    ease = carry_ease(carry.left);
                    carry.left = (carry.left - dt).max(0.0);
                }
            }
            let mut pos = tf.translation;
            // Locked, the hero faces the cursor and WASD moves him under it, so he backpedals and
            // strafes as a player does; unlocked (or driven), he faces where he runs.
            let face = (rig.mode == CamMode::Fixed
                && auto.is_none()
                && *phase.get() == Phase::Playing
                && run.dead_t.is_none())
            .then(|| aim.0.map(|a| flat(a - pos)))
            .flatten()
            .filter(|d| d.length() > 0.5)
            .map(|d| d.normalize());
            if let Some(look) = face {
                let target = Quat::from_rotation_y(yaw_toward(look));
                tf.rotation = tf.rotation.slerp(target, (dt * 20.0).min(1.0));
            }
            if dir != Vec3::ZERO && dt > 0.0 {
                let dir = dir.normalize();
                let speed = run.stats().speed * run.haste_mult() * ease;
                pos += dir * speed * dt;
                mv.speed = speed;
                mv.flags = match face {
                    Some(look) => gait(look, dir),
                    None => {
                        let target = Quat::from_rotation_y(yaw_toward(dir));
                        tf.rotation = tf.rotation.slerp(target, (dt * 20.0).min(1.0));
                        move_flags::FORWARD
                    }
                };
            } else if mv.speed != 0.0 || mv.flags != 0 {
                mv.speed = 0.0;
                mv.flags = 0;
            }
            let map = selection.stage_def();
            let center = arena(map);
            let off = flat(pos - center);
            if off.length() > map.leash * 2.0 {
                // Just entered another battleground: start at its centre.
                pos = center;
            } else if off.length() > map.leash {
                let held = center + off.normalize() * map.leash;
                pos.x = held.x;
                pos.z = held.z;
            }
            let ground = world.terrain_height_under(pos).unwrap_or(pos.y);
            let playing = *phase.get() == Phase::Playing && run.dead_t.is_none();
            // Space hops: lift off the terrain, then gravity brings the hero back down.
            if playing && keys.just_pressed(KeyCode::Space) && hm.vy == 0.0 && hm.air == 0.0 {
                hm.vy = JUMP_SPEED;
            }
            if playing && (hm.vy != 0.0 || hm.air > 0.0) {
                hm.vy -= GRAVITY * dt;
                hm.air += hm.vy * dt;
                if hm.air <= 0.0 {
                    hm.air = 0.0;
                    hm.vy = 0.0;
                }
            } else {
                hm.air = 0.0;
                hm.vy = 0.0;
            }
            pos.y = ground + hm.air;
            // Feed the animation driver: `vertical_speed`'s sign and the `FALLING` flag are what
            // select the Jump (37/38) / Fall (40) animation (`creature_anim::select`).
            mv.vertical_speed = hm.vy;
            if hm.vy != 0.0 {
                mv.flags |= move_flags::FALLING;
            }
            if tf.translation != pos {
                tf.translation = pos;
            }
            pos
        }
        Err(_) => {
            let mut p = arena(selection.stage_def());
            if let Some(h) = world.terrain_height_under(p) {
                p.y = h;
            }
            p
        }
    };
    // The terrain streams and the loading screen waits on the ground under this.
    player.active = true;
    player.detached = false;
    player.settling = false;
    player.world_stale = false;
    player.pos = focus;
}

/// Let go of the mouse buttons when the page loses focus or the pointer leaves it. A release that
/// lands elsewhere (alt-tab mid-drag, a browser or OS popup) never reaches the game, so the button
/// would read as held and the unlocked camera would swing with every move until both buttons were
/// clicked again. Survivors only: the engine's mouse-look keeps its buttons across a deactivate,
/// as 1.12.1 does.
pub(super) fn drop_stray_buttons(
    mut focus: MessageReader<bevy::window::WindowFocused>,
    mut left: MessageReader<bevy::window::CursorLeft>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    let lost = focus.read().any(|f| !f.focused);
    if lost | (left.read().count() > 0) {
        mouse.release_all();
    }
}

/// How far above the hero, along the view, the locked camera's cutaway starts: nearer the lens
/// than this, nothing draws. From straight above, a tree's canopy over the hero sits between the
/// lens and the hero and hid the fight (Elwynn's oaks, device test 2026-10-30); the static pass
/// draws trees in bulk, so a per-tree fade cannot reach them, but the near plane cuts them all.
const CUTAWAY_YD: f32 = 9.0;

/// The locked camera's cutaway: its projection's near plane pushed to [`CUTAWAY_YD`] short of the
/// hero, after `benilla_world::view::stamp_near_clip` stamps the `nearclip` setting (which every
/// other reader keeps reading). The menu's orbit and the unlocked camera keep the setting's plane.
pub(super) fn cutaway(
    rig: Res<CamRig>,
    phase: Res<State<Phase>>,
    hero: Query<&Transform, (With<Hero>, Without<WorldCamera>)>,
    mut cams: Query<(&Transform, &mut Projection), With<WorldCamera>>,
) {
    let in_run = !matches!(phase.get(), Phase::Menu | Phase::GameOver);
    if rig.mode != CamMode::Fixed || !in_run {
        return;
    }
    let Ok(hero) = hero.single() else { return };
    let head = hero.translation + Vec3::Y * 1.5;
    for (tf, mut proj) in &mut cams {
        let near = (tf.translation.distance(head) - CUTAWAY_YD).max(0.1);
        let Projection::Perspective(p) = proj.as_ref() else {
            continue;
        };
        if (p.near - near).abs() > 0.01 {
            if let Projection::Perspective(p) = &mut *proj {
                p.near = near;
            }
        }
    }
}

/// What stands between the locked camera and the hero draws see-through
/// (`benilla_world::model_fade::OcclusionFade`): trees whose bounding sphere, at [`SEE_REACH`] of
/// its radius, crosses the line from the lens to the hero's head, at [`SEE_ALPHA`]. On the tablet
/// this cleared Elwynn's oaks at 20 % for ~8 % of the frame rate (83 → 76 fps, 2026-10-30): the
/// trees are entities now, outside the static pass and the merge.
const SEE_ALPHA: f32 = 0.2;
const SEE_REACH: f32 = 1.0;

pub(super) fn see_through(
    rig: Res<CamRig>,
    phase: Res<State<Phase>>,
    hero: Query<&Transform, With<Hero>>,
    mut occlusion: ResMut<benilla_world::model_fade::OcclusionFade>,
) {
    let in_run = !matches!(phase.get(), Phase::Menu | Phase::GameOver);
    let target = (rig.mode == CamMode::Fixed && in_run)
        .then(|| hero.single().ok().map(|h| h.translation + Vec3::Y * 1.5))
        .flatten();
    // `WOW_SEE_ALPHA` / `WOW_SEE_REACH` (`android-env.txt` on the device) tune it without a build.
    static TUNE: std::sync::OnceLock<(f32, f32)> = std::sync::OnceLock::new();
    let (alpha, reach) = *TUNE.get_or_init(|| {
        let var = |k: &str, d: f32| {
            crate::webenv::var(k)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(d)
        };
        (
            var("WOW_SEE_ALPHA", SEE_ALPHA),
            var("WOW_SEE_REACH", SEE_REACH),
        )
    });
    let want = benilla_world::model_fade::OcclusionFade {
        target,
        alpha,
        reach,
    };
    if *occlusion != want {
        *occlusion = want;
    }
}

/// A slow orbit at the menu; in play, top-down until `C` unlocks it to swing and tilt.
pub(super) fn place_camera(
    time: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    mut rig: ResMut<CamRig>,
    mut wheel: MessageReader<MouseWheel>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    player: Res<crate::player::Player>,
    world: WorldPoint,
    hero: Query<&Transform, (With<Hero>, Without<WorldCamera>)>,
    mut cams: Query<&mut Transform, With<WorldCamera>>,
) {
    // `C` (or the touch camera button, which presses it) unlocks the camera to swing and tilt;
    // locking it again keeps the view the player set. It used to snap back to top-down so
    // world-relative WASD lined up with the screen; movement follows the camera now
    // ([`move_dir`]), and the user asked for the view to stay (tablet feedback, 2026-10-31).
    if keys.just_pressed(KeyCode::KeyC) {
        toggle_lock(&mut rig);
    }
    // Every run starts from the default view; the menu's own orbit ignores yaw and pitch.
    if *phase.get() == Phase::Menu {
        rig.yaw = 0.0;
        rig.pitch = FIXED_PITCH;
        rig.mode = CamMode::Fixed;
    }
    // With no hero yet, the focus `drive_hero` grounded.
    let focus = hero.single().map_or(player.pos, |t| t.translation);
    // Multiplicative, so a notch moves as far by eye close in as far out; all the way down to an
    // over-the-shoulder view.
    for w in wheel.read() {
        let notches = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y * 0.02,
        };
        rig.dist = (rig.dist * (1.0 - 0.12 * notches).clamp(0.5, 1.5)).clamp(ZOOM_MIN, ZOOM_MAX);
    }
    let (eye, look) = if *phase.get() == Phase::Menu {
        rig.orbit += time.delta_secs() * 0.2;
        let d = 7.0;
        (
            focus + Vec3::new(rig.orbit.sin() * d, 2.4, rig.orbit.cos() * d),
            focus + Vec3::Y * 1.1,
        )
    } else {
        // Unlocked, either mouse button drags the camera around the hero, as in WoW.
        if rig.mode == CamMode::Orbit && mouse.any_pressed([MouseButton::Left, MouseButton::Right])
        {
            rig.yaw -= motion.delta.x * ORBIT_RATE;
            rig.pitch = (rig.pitch + motion.delta.y * ORBIT_RATE).clamp(PITCH_MIN, PITCH_MAX);
        }
        let (sy, cy) = rig.yaw.sin_cos();
        let (sp, cp) = rig.pitch.sin_cos();
        let look = focus + Vec3::Y * 1.4;
        let mut eye = look + Vec3::new(cp * sy, sp, cp * cy) * rig.dist;
        // Never under the ground: a low tilt on a slope would bury the lens.
        if let Some(h) = world.terrain_height_under(eye) {
            eye.y = eye.y.max(h + 0.6);
        }
        (eye, look)
    };
    let placed = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    for mut cam in &mut cams {
        *cam = placed;
    }
}

/// `WOW_SURVIVORS_GOD=1`: the hero takes no damage. A test lever, so an unattended run
/// (`super::auto`) reaches the final boss and the victory path behind it.
pub(super) fn god_mode() -> bool {
    static GOD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *GOD.get_or_init(|| crate::webenv::var("WOW_SURVIVORS_GOD").as_deref() == Some("1"))
}

/// Damage the hero takes from one blow, after immunity, armour and the shield; returns what got
/// through.
pub(super) fn hurt_hero(run: &mut Run, raw: f32) -> f32 {
    if run.immune_t > 0.0 || run.dead_t.is_some() || god_mode() {
        return 0.0;
    }
    run.last_hurt = run.elapsed;
    let stats = run.stats();
    let mut dmg = (raw - stats.armor).max(1.0);
    if run.shield_t > 0.0 && run.shield > 0.0 {
        let absorbed = dmg.min(run.shield);
        run.shield -= absorbed;
        dmg -= absorbed;
    }
    run.hp -= dmg;
    run.min_hp_frac = run.min_hp_frac.min((run.hp / stats.max_hp.max(1.0)).max(0.0));
    dmg
}

/// Health a second every hero mends, on top of its Regen stat.
const BASE_REGEN: f32 = 0.4;
/// After this long without a blow, the hero mends [`CALM_REGEN`] of its pool a second more.
const CALM_SECS: f32 = 4.0;
const CALM_REGEN: f32 = 0.015;

/// The clock, regeneration, the timed buffs and their visuals, death and victory.
pub(super) fn hero_vitals(
    time: Res<Time>,
    mut run: ResMut<Run>,
    mut next: ResMut<NextState<Phase>>,
    mut hero: Query<(Entity, &Guid, &mut ObjectStore, &mut Marks), With<Hero>>,
    mut fields: MessageWriter<FieldChanged>,
    mut fx: Fx,
) {
    let dt = time.delta_secs();
    let stats = run.stats();
    let Ok((entity, guid, mut store, mut marks)) = hero.single_mut() else {
        return;
    };
    if let Some(t) = run.dead_t.as_mut() {
        *t += dt;
        if *t > 3.0 {
            next.set(Phase::GameOver);
        }
        return;
    }
    run.elapsed += dt;
    let r = &mut *run;
    for t in [&mut r.shield_t, &mut r.immune_t, &mut r.thorns_t] {
        *t = (*t - dt).max(0.0);
    }
    // Every hero mends a little, and faster once out of reach for a while: early scratches must
    // not stay for the whole run (the tablet's balance runs, 2026-10-31: a Hunter sat at 47/162
    // for fifteen seconds before the first ring took it).
    let mut heal = (stats.regen + BASE_REGEN) * dt;
    if run.elapsed - run.last_hurt > CALM_SECS {
        heal += CALM_REGEN * stats.max_hp * dt;
    }
    if run.heal_t > 0.0 {
        heal += run.heal_rate * dt.min(run.heal_t);
        run.heal_t = (run.heal_t - dt).max(0.0);
    }
    run.hp = (run.hp + heal).min(stats.max_hp);

    // The buffs show as their own state kits: the shield bubble, the immunity glow, the orbs.
    let mut shown: Vec<u32> = Vec::new();
    if run.shield_t > 0.0 && run.shield > 0.0 {
        shown.push(17);
    }
    if run.immune_t > 0.0 {
        shown.push(642);
    }
    if run.thorns_t > 0.0 {
        shown.push(run.thorns_spell);
    }
    if run.heal_t > 0.0 {
        shown.push(774);
    }
    for owned in &run.abilities {
        if owned.def.key == "lightning_shield" {
            shown.push(owned.def.spell);
        }
    }
    if run.passives.iter().any(|(p, _)| p.key == "renew") {
        shown.push(139);
    }
    marks.set(&shown);
    marks.flush(entity, guid.0, &mut store, &mut fields);

    if run.hp <= 0.0 {
        run.hp = 0.0;
        run.dead_t = Some(0.0);
        units::kill_unit(&mut store);
        marks.clear();
        marks.flush(entity, guid.0, &mut store, &mut fields);
    } else if run.won {
        // A last flourish before the results.
        fx.cast(entity, super::secrets::VICTORY_FLOURISH, &[entity], None);
        next.set(Phase::GameOver);
    }
}

/// The ground-plane distance from the hero, the measure every range in the mode uses.
/// The movement flags for running `dir` while facing `look`: forward, backpedal or strafe, by
/// which quarter the run falls in, so the walk, backpedal and strafe animations play.
fn gait(look: Vec3, dir: Vec3) -> u32 {
    let ahead = look.dot(dir);
    // Positive when `dir` is to the hero's left: WoW faces −Z with +X on the right.
    let left = look.cross(dir).y;
    let quarter = std::f32::consts::FRAC_1_SQRT_2;
    if ahead >= quarter {
        move_flags::FORWARD
    } else if ahead <= -quarter {
        move_flags::BACKWARD
    } else if left > 0.0 {
        move_flags::STRAFE_LEFT
    } else {
        move_flags::STRAFE_RIGHT
    }
}

pub(super) fn reach(hero: Vec3, other: Vec3) -> f32 {
    flat(other - hero).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carry_runs_full_then_eases_out() {
        assert_eq!(carry_ease(1.0), 1.0);
        assert_eq!(carry_ease(0.2), 1.0);
        assert!((carry_ease(0.1) - 0.5).abs() < 1e-6);
        assert_eq!(carry_ease(0.0), 0.0);
    }

    #[test]
    fn a_pick_carries_the_heading_and_standing_still_carries_nothing() {
        let mut world = World::new();
        world.insert_resource(MoveCarry {
            last: Vec3::X,
            ..default()
        });
        world.run_system_once(remember_heading).unwrap();
        world.run_system_once(carry_heading).unwrap();
        let c = world.resource::<MoveCarry>();
        assert_eq!((c.dir, c.left), (Vec3::X, CARRY_SECS));

        world.insert_resource(MoveCarry::default());
        world.run_system_once(remember_heading).unwrap();
        world.run_system_once(carry_heading).unwrap();
        assert_eq!(world.resource::<MoveCarry>().left, 0.0);
    }

    /// The hero is the character the player hears from: the sound plugin seats the listener on an
    /// `Embodied` body and falls back to the camera, and the hero has none. Measured on the tablet,
    /// the hero's own `BowRelease` (vol 0.69, min 8) reached the mix at 0.049, rolled off from the
    /// camera ~34 yd away, against music at 0.4 and ambience at 0.6.
    #[test]
    fn the_listener_hears_from_the_hero_not_the_camera() {
        let mut app = App::new();
        app.insert_resource(crate::sound::SoundConfig::default());
        // Where the fallback seats it: the camera, 30 yd back and up.
        let cam_rot = Quat::from_rotation_x(-FIXED_PITCH);
        app.insert_resource(crate::sound::AudioListener {
            pos: Vec3::new(0.0, 25.0, 17.0),
            rot: cam_rot,
        });
        let feet = Vec3::new(3.0, 1.0, -2.0);
        app.world_mut().spawn((
            Hero { vy: 0.0, air: 0.0 },
            Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(0.7)),
        ));
        app.add_systems(Update, seat_listener);
        app.update();
        let listener = app.world().resource::<crate::sound::AudioListener>();
        let ear = feet + Vec3::Y * crate::player::head_height(None, 1.0);
        assert!(
            listener.pos.distance(ear) < 1e-4,
            "listener at {:?}, the hero's ear at {ear:?}",
            listener.pos
        );
        // The camera's facing stays: on a top-down view left and right are the screen's, not the
        // hero's, which turns with every shot.
        assert_eq!(listener.rot, cam_rot);
    }

    /// The `SoundListenerAtCharacter` CVar off keeps the camera's ear, as in the client.
    #[test]
    fn the_cvar_off_keeps_the_camera_listener() {
        let mut app = App::new();
        app.insert_resource(crate::sound::SoundConfig {
            listener_at_character: false,
            ..default()
        });
        let cam = Vec3::new(0.0, 25.0, 17.0);
        app.insert_resource(crate::sound::AudioListener {
            pos: cam,
            rot: Quat::IDENTITY,
        });
        app.world_mut()
            .spawn((Hero { vy: 0.0, air: 0.0 }, Transform::default()));
        app.add_systems(Update, seat_listener);
        app.update();
        assert_eq!(app.world().resource::<crate::sound::AudioListener>().pos, cam);
    }

    #[test]
    fn locking_keeps_the_view() {
        let mut rig = CamRig {
            yaw: 1.2,
            pitch: 0.9,
            mode: CamMode::Orbit,
            ..default()
        };
        toggle_lock(&mut rig);
        assert_eq!((rig.mode, rig.yaw, rig.pitch), (CamMode::Fixed, 1.2, 0.9));
        toggle_lock(&mut rig);
        assert_eq!((rig.mode, rig.yaw, rig.pitch), (CamMode::Orbit, 1.2, 0.9));
    }

    #[test]
    fn movement_follows_the_camera() {
        let up = Vec3::new(0.0, 0.0, -1.0);
        // The default view: exactly the old world-relative W.
        assert!(move_dir(up, 0.0).abs_diff_eq(up, 1e-6));
        // A quarter turn rotates it the way the orbit mode always has.
        let turned = move_dir(up, std::f32::consts::FRAC_PI_2);
        assert!(turned.abs_diff_eq(Vec3::new(-1.0, 0.0, 0.0), 1e-6), "{turned}");
    }
    use bevy::ecs::system::RunSystemOnce;
    use bevy::window::{CursorLeft, WindowFocused};

    fn held_left() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<WindowFocused>>();
        world.init_resource::<Messages<CursorLeft>>();
        let mut mouse = ButtonInput::<MouseButton>::default();
        mouse.press(MouseButton::Left);
        world.insert_resource(mouse);
        world
    }

    fn left_held(world: &World) -> bool {
        world
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    }

    #[test]
    fn a_held_button_is_dropped_when_the_page_loses_focus() {
        let mut world = held_left();
        world.run_system_once(drop_stray_buttons).unwrap();
        assert!(left_held(&world), "nothing happened: the drag goes on");
        world.write_message(WindowFocused {
            window: Entity::PLACEHOLDER,
            focused: true,
        });
        world.run_system_once(drop_stray_buttons).unwrap();
        assert!(left_held(&world), "gaining focus is no release");
        world.write_message(WindowFocused {
            window: Entity::PLACEHOLDER,
            focused: false,
        });
        world.run_system_once(drop_stray_buttons).unwrap();
        assert!(!left_held(&world));
    }

    #[test]
    fn a_held_button_is_dropped_when_the_pointer_leaves_the_page() {
        let mut world = held_left();
        world.write_message(CursorLeft {
            window: Entity::PLACEHOLDER,
        });
        world.run_system_once(drop_stray_buttons).unwrap();
        assert!(!left_held(&world));
    }
}
