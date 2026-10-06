//! Highest/lowest press ledger and retained keyboard-CV hold stages.
//!
//! The source keyboard reads one resistor chain from both ends: VCO-1 always takes
//! the highest held key and VCO-2 the lowest; one key owns both. On final release
//! both pre-portamento targets and their owners remain held (`research:instruments/sh-7.md`
//! §§4.1–4.2). Host duplicate semantics do not exist in the hardware, so this module
//! follows the approved software contract: every NoteOn is a press, oldest wins an
//! equal key, and an ID is authoritative only when both sides carry one.

/// Chosen capacity: comfortably beyond two hands while remaining fixed on the audio
/// thread. Exhaustion evicts only the oldest press owning neither extreme.
pub const CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteId {
    pub voice_id: Option<i32>,
    pub channel: u8,
    pub key: u8,
}

impl NoteId {
    pub fn matches(self, voice_id: Option<i32>, channel: u8, key: u8) -> bool {
        match (self.voice_id, voice_id) {
            (Some(a), Some(b)) => a == b,
            _ => self.channel == channel && self.key == key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    pub id: NoteId,
    /// Direct oscillator offset, never part of high/low ordering or filter tracking.
    pub tuning_semitones: f32,
    /// The note-on's velocity, `0..=1`. The machine's keyboard had none; it is a routable source.
    ///
    /// **Held beside the identity rather than inside it.** A press is found by voice id, or by
    /// channel and key, and a note-off carries a velocity of its own that has no reason to equal
    /// its note-on's — so putting velocity in [`NoteId`] would either break that matching or need
    /// an exception written into it.
    pub velocity: f32,
    sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Owner {
    pub id: NoteId,
    pub tuning_semitones: f32,
    /// The owning press's note-on velocity, `0..=1`; full before the first press, so the standard
    /// Velocity an owner no press has set publishes is rest, not `−1`.
    ///
    /// **It belongs to the press, so it follows the retained owner exactly as pitch does.** That
    /// answers "which press's velocity" on a paraphonic keyboard without inventing a rule, and it
    /// means a handoff cannot borrow a released press's velocity.
    pub velocity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Assignment {
    pub high: Option<Owner>,
    pub low: Option<Owner>,
}

/// Internal identities for the retained hold capacitors. Host note identity is not unique for
/// ID-less duplicate presses, so ownership must follow the accepted press rather than `NoteId`.
#[derive(Debug, Clone, Copy, Default)]
struct RetainedSequences {
    high: Option<u64>,
    low: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    pub accepted: bool,
    pub gate_opened: bool,
    pub gate_closed: bool,
    pub high_changed: bool,
    pub low_changed: bool,
    pub cut: bool,
}

#[derive(Debug, Clone)]
pub struct Keyboard {
    presses: [Option<Press>; CAPACITY],
    len: usize,
    next_sequence: u64,
    retained: Assignment,
    retained_sequences: RetainedSequences,
    default_owner: Owner,
}

impl Keyboard {
    pub const fn new(default_key: u8, default_channel: u8) -> Self {
        let owner = Owner {
            id: NoteId {
                voice_id: None,
                channel: default_channel,
                key: default_key,
            },
            tuning_semitones: 0.0,
            // Full until a press sets it: an envelope the Trigger input fires with no key ever
            // pressed must not publish the standard Velocity's `−1`.
            velocity: 1.0,
        };
        Self {
            presses: [None; CAPACITY],
            len: 0,
            next_sequence: 0,
            retained: Assignment {
                high: Some(owner),
                low: Some(owner),
            },
            retained_sequences: RetainedSequences {
                high: None,
                low: None,
            },
            default_owner: owner,
        }
    }

    pub fn reset(&mut self) {
        self.presses = [None; CAPACITY];
        self.len = 0;
        self.next_sequence = 0;
        self.retained = Assignment {
            high: Some(self.default_owner),
            low: Some(self.default_owner),
        };
        self.retained_sequences = RetainedSequences::default();
    }

    pub fn held(&self) -> usize {
        self.len
    }
    pub fn is_held(&self) -> bool {
        self.len != 0
    }
    pub fn assignment(&self) -> Assignment {
        self.retained
    }

    fn owner(p: Press) -> Owner {
        Owner {
            id: p.id,
            tuning_semitones: p.tuning_semitones,
            velocity: p.velocity,
        }
    }

    fn extremes(&self) -> (Option<usize>, Option<usize>) {
        let mut high: Option<usize> = None;
        let mut low: Option<usize> = None;
        for i in 0..self.len {
            let p = self.presses[i].unwrap();
            if high.is_none_or(|j| p.id.key > self.presses[j].unwrap().id.key) {
                high = Some(i);
            }
            if low.is_none_or(|j| p.id.key < self.presses[j].unwrap().id.key) {
                low = Some(i);
            }
            // Strict comparisons preserve the oldest equal-key press.
        }
        (high, low)
    }

    fn current_from_live(&self) -> (Assignment, RetainedSequences) {
        let (high, low) = self.extremes();
        (
            Assignment {
                high: high.map(|i| Self::owner(self.presses[i].unwrap())),
                low: low.map(|i| Self::owner(self.presses[i].unwrap())),
            },
            RetainedSequences {
                high: high.map(|i| self.presses[i].unwrap().sequence),
                low: low.map(|i| self.presses[i].unwrap().sequence),
            },
        )
    }

    fn remove_at(&mut self, at: usize) {
        for i in at..self.len - 1 {
            self.presses[i] = self.presses[i + 1];
        }
        self.len -= 1;
        self.presses[self.len] = None;
    }

    fn find(&self, voice_id: Option<i32>, channel: u8, key: u8) -> Option<usize> {
        (0..self.len).find(|&i| self.presses[i].unwrap().id.matches(voice_id, channel, key))
    }

    fn apply_live_assignment(&mut self, before: Assignment) -> Outcome {
        let (live, sequences) = self.current_from_live();
        if let Some(v) = live.high {
            self.retained.high = Some(v);
            self.retained_sequences.high = sequences.high;
        }
        if let Some(v) = live.low {
            self.retained.low = Some(v);
            self.retained_sequences.low = sequences.low;
        }
        Outcome {
            high_changed: self.retained.high != before.high,
            low_changed: self.retained.low != before.low,
            ..Outcome::default()
        }
    }

    /// A key went down, at a velocity the voice carries as a source and nothing else reads.
    pub fn note_on(&mut self, id: NoteId, velocity: f32) -> Outcome {
        let was_empty = self.len == 0;
        let before = self.retained;
        if self.len == CAPACITY {
            let (high, low) = self.extremes();
            let victim = (0..self.len).find(|i| Some(*i) != high && Some(*i) != low);
            let Some(victim) = victim else {
                return Outcome::default();
            };
            self.remove_at(victim);
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.presses[self.len] = Some(Press {
            id,
            tuning_semitones: 0.0,
            velocity,
            sequence,
        });
        self.len += 1;
        let mut out = self.apply_live_assignment(before);
        out.accepted = true;
        out.gate_opened = was_empty;
        out
    }

    pub fn note_off(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> Outcome {
        self.retire(voice_id, channel, key, false)
    }

    pub fn choke(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> Outcome {
        self.retire(voice_id, channel, key, true)
    }

    fn retire(&mut self, voice_id: Option<i32>, channel: u8, key: u8, choke: bool) -> Outcome {
        let Some(at) = self.find(voice_id, channel, key) else {
            return Outcome::default();
        };
        let before = self.retained;
        self.remove_at(at);
        let mut out = self.apply_live_assignment(before);
        out.accepted = true;
        out.gate_closed = self.len == 0;
        out.cut = choke && self.len == 0;
        out
    }

    pub fn all_notes_off(&mut self) -> Outcome {
        let held = self.len != 0;
        self.presses = [None; CAPACITY];
        self.len = 0;
        Outcome {
            accepted: held,
            gate_closed: held,
            ..Outcome::default()
        }
    }

    pub fn set_tuning(
        &mut self,
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
        semitones: f32,
    ) -> bool {
        let Some(at) = self.find(voice_id, channel, key) else {
            return false;
        };
        self.presses[at].as_mut().unwrap().tuning_semitones = semitones;
        let press = self.presses[at].unwrap();
        let owner = Self::owner(press);
        if self.retained_sequences.high == Some(press.sequence) {
            self.retained.high = Some(owner);
        }
        if self.retained_sequences.low == Some(press.sequence) {
            self.retained.low = Some(owner);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(v: i32, channel: u8, key: u8) -> NoteId {
        NoteId {
            voice_id: Some(v),
            channel,
            key,
        }
    }

    #[test]
    fn highest_lowest_and_one_key_doubling_are_press_ordered() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 1, 60), 0.0);
        assert_eq!(k.assignment().high, k.assignment().low);
        k.note_on(id(2, 2, 72), 0.0);
        k.note_on(id(3, 3, 48), 0.0);
        assert_eq!(k.assignment().high.unwrap().id.key, 72);
        assert_eq!(k.assignment().low.unwrap().id.key, 48);
        k.note_on(id(4, 4, 72), 0.0);
        assert_eq!(
            k.assignment().high.unwrap().id.voice_id,
            Some(2),
            "oldest equal key owns"
        );
    }

    #[test]
    fn a_middle_key_keeps_the_shared_gate_without_owning_pitch() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 0, 48), 0.0);
        k.note_on(id(2, 0, 72), 0.0);
        let middle = k.note_on(id(3, 0, 60), 0.0);
        assert!(middle.accepted && !middle.high_changed && !middle.low_changed);
        let release = k.note_off(Some(3), 0, 60);
        assert!(
            release.accepted
                && !release.gate_closed
                && !release.high_changed
                && !release.low_changed
        );
        k.note_on(id(4, 0, 60), 0.0);
        k.note_off(Some(1), 0, 48);
        let high = k.note_off(Some(2), 0, 72);
        assert!(!high.gate_closed);
        assert_eq!(k.assignment().high.unwrap().id.key, 60);
        assert_eq!(k.assignment().low.unwrap().id.key, 60);
    }

    #[test]
    fn release_collapses_both_targets_and_final_release_holds_them() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 0, 48), 0.0);
        k.note_on(id(2, 0, 72), 0.0);
        let o = k.note_off(Some(2), 0, 72);
        assert!(o.high_changed && !o.gate_closed);
        assert_eq!(k.assignment().high.unwrap().id.key, 48);
        assert_eq!(k.assignment().low.unwrap().id.key, 48);
        let held = k.assignment();
        k.note_off(Some(1), 0, 48);
        assert_eq!(
            k.assignment(),
            held,
            "the two CV hold capacitors retain final targets"
        );
    }

    #[test]
    fn duplicates_are_distinct_and_handoff_tuning_without_a_pitch_event() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 1, 60), 0.0);
        let o = k.note_on(id(2, 2, 60), 0.0);
        assert!(!o.high_changed && !o.low_changed && o.accepted);
        k.set_tuning(Some(2), 2, 60, 0.75);
        let o = k.note_off(Some(1), 1, 60);
        assert!(o.high_changed && o.low_changed);
        assert_eq!(k.assignment().high.unwrap().tuning_semitones, 0.75);
        assert!(!o.gate_closed);
    }

    #[test]
    fn matching_and_exhaustion_never_evict_an_extreme_owner() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 0, 1), 0.0);
        k.note_on(id(2, 0, 126), 0.0);
        for n in 0..CAPACITY + 10 {
            k.note_on(id(100 + n as i32, 0, 64), 0.0);
        }
        assert_eq!(k.assignment().low.unwrap().id.key, 1);
        assert_eq!(k.assignment().high.unwrap().id.key, 126);
        assert_eq!(k.held(), CAPACITY);
        assert!(!k.note_off(Some(9999), 0, 64).accepted);
    }

    #[test]
    fn idless_events_choose_the_oldest_match() {
        let mut k = Keyboard::new(60, 0);
        k.note_on(id(1, 4, 60), 0.0);
        k.note_on(id(2, 4, 60), 0.0);
        k.note_off(None, 4, 60);
        assert_eq!(k.assignment().high.unwrap().id.voice_id, Some(2));
    }

    #[test]
    fn idless_duplicate_tuning_changes_the_oldest_press() {
        let mut k = Keyboard::new(60, 0);
        let duplicate = NoteId {
            voice_id: None,
            channel: 4,
            key: 60,
        };
        assert!(k.note_on(duplicate, 0.0).accepted);
        assert!(k.note_on(duplicate, 0.0).accepted);

        assert!(k.set_tuning(None, 4, 60, 0.75));
        let assignment = k.assignment();
        assert_eq!(assignment.high.unwrap().tuning_semitones, 0.75);
        assert_eq!(assignment.low.unwrap().tuning_semitones, 0.75);

        // The oldest retires first, and the lanes hand to the newer press, untuned.
        assert!(k.choke(None, 4, 60).accepted);
        assert_eq!(k.assignment().high.unwrap().tuning_semitones, 0.0);
    }
}
