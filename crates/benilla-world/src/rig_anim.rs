//! The rig machinery the renderer needs: pose storage, the pose post-pass window, the rig world
//! composition and the free-running global-sequence channels. Which clip a unit plays and when a
//! rig parks are the game's policy (`creature_anim`); [`AnimParked`] lives here because it gates
//! the pose evaluator and the billboard joint pass.

use bevy::prelude::*;

mod compose;
mod global_seq;
mod pose;

pub(crate) use compose::seed_rig_rows;
pub use compose::{finalize_rig_worlds, PosePost};
pub use global_seq::GlobalSeqDrive;
pub use pose::{RigAnchor, RigFrame, RigPose};

/// Parks this rig's per-bone pose evaluation: the evaluator and the pose post-passes skip it,
/// while the sequence clocks, the driver state machine and the event scanner keep running.
#[derive(Component)]
pub struct AnimParked;

/// Carry (survivors): this rig's pose is evaluated every second frame only, on the frames
/// [`evaluates_this_frame`] picks; between them the pose holds one frame, and the compose and the
/// palette rows skip with it. A game puts it on distant crowd members; the client never adds it,
/// so every rig evaluates every frame as before.
#[derive(Component)]
pub struct AnimHalfRate;

/// Whether a half-rate rig evaluates on `frame`: alternate frames, staggered by the entity index so
/// half the throttled crowd evaluates each frame and the load stays even.
pub fn evaluates_this_frame(frame: u32, entity: Entity) -> bool {
    frame.wrapping_add(entity.index_u32()) & 1 == 0
}

/// Registers the rig machinery; `WorldPlugins` adds it, so a program with no game still poses its
/// rigs.
pub fn plugin(app: &mut App) {
    global_seq::plugin(app);
    pose::plugin(app);
    compose::plugin(app);
}

#[cfg(test)]
mod half_rate_tests {
    use super::*;

    /// Every half-rate rig evaluates on exactly one of any two consecutive frames, and an even and
    /// an odd entity index take opposite frames, so the throttled crowd splits in two.
    #[test]
    fn half_rate_rigs_alternate_and_stagger_by_index() {
        let even = Entity::from_raw_u32(10).unwrap();
        let odd = Entity::from_raw_u32(11).unwrap();
        for frame in 0..6u32 {
            assert_ne!(
                evaluates_this_frame(frame, even),
                evaluates_this_frame(frame + 1, even),
                "alternate frames"
            );
            assert_ne!(
                evaluates_this_frame(frame, even),
                evaluates_this_frame(frame, odd),
                "staggered by index"
            );
        }
        assert!(evaluates_this_frame(u32::MAX, odd), "wraps without a gap");
    }
}
