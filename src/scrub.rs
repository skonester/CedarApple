//! Dragging the seekbar: the picture follows the pointer, and the file ends up
//! on the exact frame the pointer was released over.
//!
//! Ported from Frame Player's `seek.svelte.ts` (`onSeekDown`, `pumpDragSeek`,
//! `scheduleDragSettle`, `onSeekUp`, `onSeekCancel`, the slow-seek probe),
//! GPL-3.0-or-later, Copyright (c) Evgenii Zakharov. The promises there are
//! mpv's asynchronous command replies here: every seek is sent with
//! `mpv_command_async`, and `on_reply` is the `.finally` that follows it.
//!
//! The contract, in the order it happens:
//!
//! * **Pressing** takes the displayed value away from playback. Nothing is sent
//!   yet — a click that never moves is a single exact seek on release.
//! * **The first move pauses playback** (and remembers to resume it), so the
//!   frame under the pointer is not immediately replaced by the next one.
//! * **While moving, one seek is in flight at a time.** A move that arrives
//!   while one is outstanding only marks that another is wanted; the reply
//!   sends it, aimed at wherever the pointer is *by then*. Queuing every move
//!   would leave the picture replaying a backlog long after the hand stopped.
//! * **Keyframe seeks while moving, when exact ones are slow.** An exact seek
//!   decodes from the previous keyframe; on a file where that takes longer than
//!   `SLOW_SEEK_MS` the picture would lag the pointer, so the drag uses
//!   keyframes and…
//! * **…resting for 120 ms settles on the exact frame**, so a pause in the drag
//!   shows the real frame under the pointer.
//! * **Release** seeks exactly (unless the exact frame is already on screen)
//!   and resumes playback if the drag paused it.

use crate::player::MpvHandle;
use std::time::{Duration, Instant};

/// Reply ids for `mpv_command_async`, so the event loop can route replies.
pub const REPLY_DRAG_SEEK: u64 = 0x5eec_0001;
pub const REPLY_FINAL_SEEK: u64 = 0x5eec_0002;

/// An exact seek slower than this marks the file as one where dragging should
/// use keyframes.
const SLOW_SEEK_MS: u128 = 250;
/// How long the pointer must rest before the exact frame is fetched.
pub const DRAG_SETTLE: Duration = Duration::from_millis(120);
const SAME_POS: f64 = 1e-6;

#[derive(Default)]
pub struct Scrub {
    pub dragging: bool,
    /// The release seek is in flight; the display still belongs to the drag.
    pub settling: bool,
    /// Seconds: where the pointer is, or was released.
    pub value: f64,
    moved: bool,
    in_flight: Option<(f64, bool, Instant)>,
    pending: bool,
    shown_at: Option<f64>,
    shown_exact: bool,
    resume_after: bool,
    final_started: Option<Instant>,
    file_slow_seek: bool,
    file_seek_probed: bool,
}

impl Scrub {
    /// Whether the displayed position belongs to the drag rather than to mpv.
    pub fn owns_display(&self) -> bool {
        self.dragging || self.settling
    }

    /// A new file: whether seeking is slow is a property of the file.
    pub fn reset_probe(&mut self) {
        self.file_slow_seek = false;
        self.file_seek_probed = false;
    }

    fn note_seek_cost(&mut self, started: Instant) {
        let slow = started.elapsed().as_millis() > SLOW_SEEK_MS;
        if !self.file_seek_probed {
            self.file_seek_probed = true;
            self.file_slow_seek = slow;
        } else if slow {
            self.file_slow_seek = true;
        }
    }

    /// Exact unless this is a preview on a file where exact is slow.
    pub fn want_exact(&self, is_preview: bool) -> bool {
        !self.file_slow_seek || !is_preview
    }

    pub fn begin(&mut self, seconds: f64) {
        self.dragging = true;
        self.moved = false;
        self.shown_at = None;
        self.shown_exact = false;
        self.value = seconds;
    }

    pub fn move_to(&mut self, mpv: &MpvHandle, seconds: f64) {
        if !self.dragging {
            return;
        }
        if !self.moved {
            // Pause for the drag, and only resume what the drag paused.
            if !mpv.get_flag("pause") {
                self.resume_after = true;
                mpv.set_flag("pause", true);
            }
        }
        self.moved = true;
        self.value = seconds;
        let exact = self.want_exact(true);
        self.pump(mpv, exact);
    }

    /// The pointer has rested: fetch the exact frame under it.
    pub fn settle(&mut self, mpv: &MpvHandle) {
        if self.dragging {
            self.pump(mpv, true);
        }
    }

    fn pump(&mut self, mpv: &MpvHandle, exact: bool) {
        if self.in_flight.is_some() {
            self.pending = true;
            return;
        }
        let target = self.value;
        if let Some(at) = self.shown_at
            && (at - target).abs() < SAME_POS
            && (self.shown_exact || !exact)
        {
            return;
        }
        self.in_flight = Some((target, exact, Instant::now()));
        issue_seek(mpv, target, exact, REPLY_DRAG_SEEK);
    }

    pub fn end(&mut self, mpv: &MpvHandle, seconds: f64) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.value = seconds;
        let already_there = self.shown_exact
            && self
                .shown_at
                .is_some_and(|at| (at - self.value).abs() < SAME_POS);
        if already_there {
            self.finish(mpv);
        } else {
            self.settling = true;
            self.final_started = Some(Instant::now());
            issue_seek(mpv, self.value, true, REPLY_FINAL_SEEK);
        }
    }

    pub fn cancel(&mut self, mpv: &MpvHandle) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.finish(mpv);
    }

    fn finish(&mut self, mpv: &MpvHandle) {
        self.settling = false;
        if self.resume_after {
            self.resume_after = false;
            mpv.set_flag("pause", false);
        }
    }

    /// An `MPV_EVENT_COMMAND_REPLY` for one of ours.
    pub fn on_reply(&mut self, mpv: &MpvHandle, reply: u64) {
        match reply {
            REPLY_DRAG_SEEK => {
                let Some((target, exact, started)) = self.in_flight.take() else {
                    return;
                };
                if exact {
                    self.note_seek_cost(started);
                }
                self.shown_at = Some(target);
                self.shown_exact = exact;
                let again = std::mem::take(&mut self.pending);
                if again && self.dragging {
                    let exact = self.want_exact(true);
                    self.pump(mpv, exact);
                }
            }
            REPLY_FINAL_SEEK => {
                if let Some(started) = self.final_started.take() {
                    self.note_seek_cost(started);
                }
                self.finish(mpv);
            }
            _ => {}
        }
    }
}

fn issue_seek(mpv: &MpvHandle, pos: f64, exact: bool, reply: u64) {
    let pos = format!("{:.3}", pos.max(0.0));
    let mode = if exact {
        "absolute+exact"
    } else {
        "absolute+keyframes"
    };
    mpv.command_async(reply, &["seek", &pos, mode]);
}
