//! In-app playback of MD trajectories stored alongside a run in the task
//! directory. The trajectory file is decoded off the UI thread into a
//! [`Trajectory`]; the resulting [`TrajectoryPlayback`] holds the playback
//! cursor and a scratch [`Structure`] whose positions are swapped per frame.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use nalgebra::Point3;

use crate::domain::{Structure, Trajectory};
use crate::io::trajectory::read_xtc;

/// Default playback rate (frames per second).
pub const DEFAULT_PLAYBACK_FPS: f32 = 15.0;

/// A decoded trajectory bound to an entry, plus the UI playback state. Rendering
/// only happens while the bound entry is active (see the workspace renderer).
pub struct TrajectoryPlayback {
    /// Entry this trajectory belongs to.
    pub entry_id: u64,
    pub trajectory: Trajectory,
    /// The entry's topology with the current frame's coordinates applied; this
    /// is what the viewport renders during playback.
    pub scratch: Structure,
    pub current_frame: usize,
    pub playing: bool,
    pub fps: f32,
    /// egui time (seconds) at which `current_frame` was last advanced.
    pub last_advance_secs: f64,
    /// Camera framing computed once from the base structure and held fixed, so
    /// the view does not drift/zoom as the system diffuses between frames.
    pub view_center: Point3<f32>,
    pub view_radius: f32,
}

impl TrajectoryPlayback {
    pub fn frame_count(&self) -> usize {
        self.trajectory.frame_count()
    }

    /// Apply `current_frame`'s coordinates to the scratch structure.
    pub fn sync_scratch(&mut self) {
        self.trajectory
            .apply_frame(self.current_frame, &mut self.scratch.atoms);
    }

    /// Jump to `frame` (clamped) and refresh the scratch structure.
    pub fn set_frame(&mut self, frame: usize) {
        let last = self.frame_count().saturating_sub(1);
        self.current_frame = frame.min(last);
        self.sync_scratch();
    }

    /// Advance one frame (wrapping) and refresh the scratch structure.
    pub fn advance_frame(&mut self) {
        let count = self.frame_count();
        if count == 0 {
            return;
        }
        self.current_frame = (self.current_frame + 1) % count;
        self.sync_scratch();
    }
}

/// An in-flight background decode of an entry's trajectory file.
pub struct RunningTrajectoryLoad {
    pub entry_id: u64,
    /// Delivers the decoded trajectory, or an error message, once decoding ends.
    pub receiver: Receiver<Result<Trajectory, String>>,
    /// The entry's base structure (topology), captured at spawn time; used to
    /// build the playback scratch and fixed view once the trajectory arrives.
    pub base_structure: Structure,
    /// Whether the unit cell is shown, for the fixed-view computation.
    pub include_cell: bool,
}

/// Spawn a background thread that decodes `path` into a [`Trajectory`]. The
/// caller stores the returned handle and polls its `receiver` on the UI thread.
pub fn spawn_trajectory_load(
    entry_id: u64,
    path: PathBuf,
    base_structure: Structure,
    include_cell: bool,
) -> RunningTrajectoryLoad {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let result = read_xtc(&path).map_err(|error| format!("{error:#}"));
        let _ = sender.send(result);
    });
    RunningTrajectoryLoad {
        entry_id,
        receiver,
        base_structure,
        include_cell,
    }
}
