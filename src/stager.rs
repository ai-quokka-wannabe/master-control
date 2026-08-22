/*
    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! The acceptance window, the dedupe, and the silence rules - TOPOLOGY.md § One tick, across
//! the wire, as a pure state machine with no socket anywhere near it.
//!
//! Every intent that reaches the wire either reaches the world or is refused **on the record**:
//! `feed` answers a verdict for every candidate it judged, because a replay that cannot say
//! what happened to an action cannot explain the world it reproduces. Nothing here logs or
//! sends; the heartbeat owns those.

use crate::link_dll::{ACTIONS_REPEAT_TICKS, Actions};
use std::collections::HashMap;

/// The twelve bytes of intent, staged or applied.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Intent {
    pub forward_speed: f32,
    pub turn_rate: f32,
    pub vocalisation: f32,
}

/// What happened to one candidate intent - the record the logs are owed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Verdict {
    /// Staged for its tick. Replaces an earlier intent staged for the same (creature, tick) -
    /// latest wins - which is also what makes the piggybacked resend free to process.
    Accepted { creature_id: u32, intent_tick: u64 },
    /// The piggybacked resend of an intent this creature already stepped with: the silence rule
    /// working as designed, idempotent, and not worth a line - a log that names every honest
    /// resend as a refusal buries the refusals that matter.
    AlreadyApplied { creature_id: u32, intent_tick: u64 },
    /// The piggyback on a stream's first ACTIONS, naming the tick the creature stepped before
    /// any intent could have reached it: a host learns of a tick from its telling, so its first
    /// word is always about the tick after the first it was told, and the resend beside it
    /// describes a step that was rightly coasted. Not a loss; on the record as what it is,
    /// never as a refusal.
    BeforeFirstIntent { creature_id: u32, intent_tick: u64 },
    /// Arrived after its tick was stepped, and was never applied: a real loss. Refused, on the
    /// record.
    RefusedStale {
        creature_id: u32,
        intent_tick: u64,
        next_tick: u64,
    },
    /// Tagged beyond the window. An interval refuses both directions of nonsense.
    RefusedFuture {
        creature_id: u32,
        intent_tick: u64,
        next_tick: u64,
    },
    /// Sent by a connection that does not own the creature - sender-owns-creature, refused.
    RefusedNotOwner { creature_id: u32, sender: u64 },
    /// Sent for an identity nobody wears: intents steer bodies, and there is no body.
    RefusedNotEmbodied { creature_id: u32, sender: u64 },
}

/// How a step's intent came to be - the applied half of the record.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Applied {
    /// The staged intent for exactly this tick.
    Fresh(Intent),
    /// A silent network: the last accepted intent, re-applied within its bounded budget.
    Repeated(Intent),
    /// Silence past the budget, or nothing ever accepted: zeroed coast.
    Coasted,
}

struct CreatureStaging {
    /// The connection that owns this creature's intent stream - the roster's word, mirrored
    /// here by [`ActionStager::reassign`] so a stale stream can never outlive its owner.
    owner: u64,
    /// Accepted intents by the tick they are staged for. Two entries at most in practice -
    /// the window is two ticks wide - but a map states the rule rather than an optimisation.
    staged: HashMap<u64, Intent>,
    last_accepted: Option<Intent>,
    repeat_budget: u32,
    /// The tick whose fresh intent was last applied, so a resend of it is known for what it is.
    last_applied_tick: Option<u64>,
    /// Whether any intent has ever been accepted on this stream - before the first, a stale
    /// piggyback is the first word's, not a loss.
    ever_accepted: bool,
}

/// The stager for every steerable creature. Etape 1 has exactly one (the scripted guest), but
/// nothing here knows that.
#[derive(Default)]
pub struct ActionStager {
    creatures: HashMap<u32, CreatureStaging>,
}

impl ActionStager {
    /// Judge one ACTIONS message from connection `sender`, with `next_tick` the tick the world
    /// will step next. The current intent and the piggybacked previous each get their own
    /// verdict through the same rules - idempotent by (creature, tick), so a resend that
    /// already landed simply lands again on top of itself.
    ///
    /// The window with integer ticks: accept `next_tick` (the step being staged for) and
    /// `next_tick + 1` (a host already past the boundary this end has not crossed yet);
    /// earlier is stale, later is refused as future rather than queued.
    pub fn feed(&mut self, sender: u64, actions: &Actions, next_tick: u64) -> Vec<Verdict> {
        let mut verdicts = Vec::with_capacity(2);

        let staging = self
            .creatures
            .entry(actions.creature_id)
            .or_insert_with(|| CreatureStaging {
                owner: sender,
                staged: HashMap::new(),
                last_accepted: None,
                repeat_budget: 0,
                last_applied_tick: None,
                ever_accepted: false,
            });
        if staging.owner != sender {
            verdicts.push(Verdict::RefusedNotOwner {
                creature_id: actions.creature_id,
                sender,
            });
            return verdicts;
        }

        let mut judge = |tick: u64, intent: Intent, piggybacked: bool| {
            let verdict = if staging.last_applied_tick == Some(tick) {
                Verdict::AlreadyApplied {
                    creature_id: actions.creature_id,
                    intent_tick: tick,
                }
            } else if piggybacked && tick < next_tick && !staging.ever_accepted {
                Verdict::BeforeFirstIntent {
                    creature_id: actions.creature_id,
                    intent_tick: tick,
                }
            } else if tick < next_tick {
                Verdict::RefusedStale {
                    creature_id: actions.creature_id,
                    intent_tick: tick,
                    next_tick,
                }
            } else if tick > next_tick.saturating_add(1) {
                Verdict::RefusedFuture {
                    creature_id: actions.creature_id,
                    intent_tick: tick,
                    next_tick,
                }
            } else {
                staging.staged.insert(tick, intent);
                staging.ever_accepted = true;
                Verdict::Accepted {
                    creature_id: actions.creature_id,
                    intent_tick: tick,
                }
            };
            verdicts.push(verdict);
        };

        // The piggybacked previous first, so a same-tick disagreement resolves in favour of the
        // fresher telling - the current intent judged second overwrites it.
        if actions.tick > 0 {
            judge(
                actions.tick - 1,
                Intent {
                    forward_speed: actions.previous_forward_speed,
                    turn_rate: actions.previous_turn_rate,
                    vocalisation: actions.previous_vocalisation,
                },
                true,
            );
        }
        judge(
            actions.tick,
            Intent {
                forward_speed: actions.desired_forward_speed,
                turn_rate: actions.desired_turn_rate,
                vocalisation: actions.vocalisation_strength,
            },
            false,
        );

        verdicts
    }

    /// The intent the world steps `tick` with, under the silence rules: fresh when staged;
    /// otherwise the last accepted intent re-applied for up to `LNK_ACTIONS_REPEAT_TICKS`;
    /// otherwise zeroed coast. A silent Program is none of these - its host sent zeroes, which
    /// arrive as a fresh intent and brake exactly as the ABI promises.
    pub fn intent_for(&mut self, creature_id: u32, tick: u64) -> Applied {
        let Some(staging) = self.creatures.get_mut(&creature_id) else {
            return Applied::Coasted;
        };

        // Anything staged for an older tick than the one being stepped is spent: it was either
        // applied at its own step or superseded, and holding it would leak.
        staging.staged.retain(|staged_tick, _| *staged_tick >= tick);

        if let Some(intent) = staging.staged.remove(&tick) {
            staging.last_accepted = Some(intent);
            staging.last_applied_tick = Some(tick);
            staging.repeat_budget = ACTIONS_REPEAT_TICKS;
            Applied::Fresh(intent)
        } else if staging.repeat_budget > 0
            && let Some(last) = staging.last_accepted
        {
            staging.repeat_budget -= 1;
            Applied::Repeated(last)
        } else {
            Applied::Coasted
        }
    }

    /// The roster's word on who steers a creature: a `REZ` (or a claim by steering) hands the
    /// intent stream to `owner`, and whatever an earlier owner staged is forgotten with it.
    pub fn reassign(&mut self, creature_id: u32, owner: u64) {
        self.creatures.insert(
            creature_id,
            CreatureStaging {
                owner,
                staged: HashMap::new(),
                last_accepted: None,
                repeat_budget: 0,
                last_applied_tick: None,
                ever_accepted: false,
            },
        );
    }

    /// A creature that left the world takes its intent stream with it.
    pub fn release(&mut self, creature_id: u32) {
        self.creatures.remove(&creature_id);
    }

    /// The dead-host liveness rule: the creature stays embodied and falls to the neutral
    /// reflex. Everything staged or remembered for this owner's creatures is zeroed, and the
    /// creatures become claimable by a reconnecting host.
    pub fn owner_died(&mut self, owner: u64) {
        self.creatures.retain(|_, staging| staging.owner != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(tick: u64, forward: f32, previous: f32) -> Actions {
        Actions {
            tick,
            creature_id: 100,
            desired_forward_speed: forward,
            desired_turn_rate: 0.0,
            vocalisation_strength: 0.0,
            previous_forward_speed: previous,
            previous_turn_rate: 0.0,
            previous_vocalisation: 0.0,
            reserved0: [0; 4],
        }
    }

    #[test]
    fn the_window_accepts_now_refuses_stale_and_refuses_future() {
        let mut stager = ActionStager::default();
        let verdicts = stager.feed(1, &actions(10, 1.0, 0.5), 10);
        assert!(
            matches!(
                verdicts[0],
                Verdict::BeforeFirstIntent { intent_tick: 9, .. }
            ),
            "the first word's piggyback names a tick no intent could have reached: not a loss"
        );
        assert!(matches!(
            verdicts[1],
            Verdict::Accepted {
                intent_tick: 10,
                ..
            }
        ));

        let verdicts = stager.feed(1, &actions(11, 1.0, 1.0), 10);
        assert!(
            matches!(
                verdicts[0],
                Verdict::Accepted {
                    intent_tick: 10,
                    ..
                }
            ),
            "the piggyback lands inside the window"
        );
        assert!(
            matches!(
                verdicts[1],
                Verdict::Accepted {
                    intent_tick: 11,
                    ..
                }
            ),
            "one past the boundary is the host ahead of us, not the future"
        );

        // A first word that is itself late is a loss, whatever its piggyback is.
        let mut late = ActionStager::default();
        let verdicts = late.feed(1, &actions(8, 1.0, 0.5), 10);
        assert!(
            matches!(verdicts[1], Verdict::RefusedStale { intent_tick: 8, .. }),
            "the grace is the piggyback's alone"
        );

        // Once a word has landed, a piggyback for a tick already stepped is a real loss again.
        let verdicts = stager.feed(1, &actions(10, 1.0, 0.5), 11);
        assert!(
            matches!(verdicts[0], Verdict::RefusedStale { intent_tick: 9, .. }),
            "after the first accepted intent, stale is stale"
        );

        let verdicts = stager.feed(1, &actions(500, 1.0, 1.0), 10);
        assert!(
            matches!(
                verdicts[1],
                Verdict::RefusedFuture {
                    intent_tick: 500,
                    ..
                }
            ),
            "a far future tick is refused, never queued"
        );
    }

    #[test]
    fn the_resend_is_idempotent_and_latest_wins() {
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 1.0, 0.0), 10);
        // The same intent re-delivered by the next message's piggyback: lands on itself.
        let _ = stager.feed(1, &actions(11, 2.0, 1.0), 10);
        match stager.intent_for(100, 10) {
            Applied::Fresh(intent) => assert!(
                (intent.forward_speed - 1.0).abs() < f32::EPSILON,
                "the piggybacked 1.0 re-landed on the original 1.0"
            ),
            other => panic!("expected a fresh intent, got {other:?}"),
        }
        // A genuinely newer telling for the same tick replaces the older one.
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 1.0, 0.0), 10);
        let _ = stager.feed(1, &actions(10, 3.0, 0.0), 10);
        match stager.intent_for(100, 10) {
            Applied::Fresh(intent) => assert!(
                (intent.forward_speed - 3.0).abs() < f32::EPSILON,
                "latest wins for the same (creature, tick)"
            ),
            other => panic!("expected a fresh intent, got {other:?}"),
        }
    }

    #[test]
    fn the_resend_of_an_applied_intent_is_silence_and_a_lost_one_is_a_refusal() {
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 1.0, 0.0), 10);
        let _ = stager.intent_for(100, 10);
        // The honest host's next message: tick 11 current, tick 10 piggybacked - already applied.
        let verdicts = stager.feed(1, &actions(11, 2.0, 1.0), 11);
        assert!(
            matches!(
                verdicts[0],
                Verdict::AlreadyApplied {
                    intent_tick: 10,
                    ..
                }
            ),
            "the resend of what was stepped with is not a refusal, got {:?}",
            verdicts[0]
        );
        assert!(matches!(
            verdicts[1],
            Verdict::Accepted {
                intent_tick: 11,
                ..
            }
        ));
        // A stale intent that was never applied - the tick 12 message arriving when 13 is next,
        // with nothing ever staged for 12 - is a real loss, and says so.
        let _ = stager.intent_for(100, 11);
        let _ = stager.intent_for(100, 12);
        let verdicts = stager.feed(1, &actions(12, 3.0, 2.0), 13);
        assert!(
            matches!(
                verdicts[1],
                Verdict::RefusedStale {
                    intent_tick: 12,
                    ..
                }
            ),
            "an intent that never landed is refused on the record, got {:?}",
            verdicts[1]
        );
    }

    #[test]
    fn a_silent_network_repeats_once_then_coasts() {
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 2.0, 0.0), 10);
        assert!(matches!(stager.intent_for(100, 10), Applied::Fresh(_)));
        match stager.intent_for(100, 11) {
            Applied::Repeated(intent) => assert!(
                (intent.forward_speed - 2.0).abs() < f32::EPSILON,
                "the last accepted intent repeats"
            ),
            other => panic!("expected a repeat, got {other:?}"),
        }
        assert!(
            matches!(stager.intent_for(100, 12), Applied::Coasted),
            "the budget is one tick, then honest coasting"
        );
        assert!(matches!(stager.intent_for(100, 13), Applied::Coasted));
    }

    #[test]
    fn a_silent_program_brakes_because_its_zeroes_arrive_fresh() {
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 2.0, 0.0), 10);
        let _ = stager.intent_for(100, 10);
        let _ = stager.feed(1, &actions(11, 0.0, 2.0), 11);
        match stager.intent_for(100, 11) {
            Applied::Fresh(intent) => assert!(
                intent.forward_speed.abs() < f32::EPSILON,
                "the host's zeroes are an intent, not silence"
            ),
            other => panic!("expected the zeroes fresh, got {other:?}"),
        }
    }

    #[test]
    fn sender_owns_creature_and_a_dead_owner_frees_it() {
        let mut stager = ActionStager::default();
        let _ = stager.feed(1, &actions(10, 1.0, 0.0), 10);
        let verdicts = stager.feed(2, &actions(10, 9.0, 9.0), 10);
        assert!(
            matches!(verdicts[0], Verdict::RefusedNotOwner { sender: 2, .. }),
            "a creature has one intent stream"
        );

        stager.owner_died(1);
        assert!(
            matches!(stager.intent_for(100, 11), Applied::Coasted),
            "the neutral reflex: embodied, zeroed"
        );
        let verdicts = stager.feed(2, &actions(12, 1.0, 1.0), 12);
        assert!(
            matches!(verdicts[1], Verdict::Accepted { .. }),
            "a reconnecting host may claim the freed creature"
        );
    }

    #[test]
    fn a_replayed_tick_takes_the_latest_word_and_the_far_ends_of_u64_are_refused_not_panicked() {
        let mut stager = ActionStager::default();
        assert!(matches!(
            stager.feed(1, &actions(10, 1.0, 0.0), 10)[1],
            Verdict::Accepted { .. }
        ));
        // The same tick again, a different word: latest wins, and the step reads the latest.
        assert!(matches!(
            stager.feed(1, &actions(10, 2.0, 0.0), 10)[1],
            Verdict::Accepted { .. }
        ));
        assert!(matches!(
            stager.intent_for(100, 10),
            Applied::Fresh(Intent { forward_speed, .. }) if forward_speed == 2.0
        ));
        // Stepped, the same tick a third time is the resend of an applied intent: silence.
        assert!(matches!(
            stager.feed(1, &actions(10, 3.0, 0.0), 11)[1],
            Verdict::AlreadyApplied { .. }
        ));
        assert!(
            matches!(stager.intent_for(100, 11), Applied::Repeated(Intent { forward_speed, .. }) if forward_speed == 2.0),
            "a replayed tick never overwrites what the world stepped with"
        );

        // Tick 0 has no previous: judged once, never underflowed.
        let verdicts = stager.feed(1, &actions(0, 1.0, 0.0), 11);
        assert_eq!(verdicts.len(), 1);
        assert!(matches!(
            verdicts[0],
            Verdict::RefusedStale { intent_tick: 0, .. }
        ));
        // u64::MAX is the far future, and its piggyback u64::MAX - 1 is too.
        let verdicts = stager.feed(1, &actions(u64::MAX, 1.0, 0.0), 11);
        assert!(matches!(verdicts[0], Verdict::RefusedFuture { .. }));
        assert!(matches!(verdicts[1], Verdict::RefusedFuture { .. }));
        // And the window's arithmetic at the top of the range does not wrap.
        let verdicts = stager.feed(1, &actions(u64::MAX, 1.0, 0.0), u64::MAX);
        assert!(matches!(verdicts[1], Verdict::Accepted { .. }));
    }
}
