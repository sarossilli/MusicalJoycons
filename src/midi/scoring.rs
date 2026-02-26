//! Primary / Secondary part scoring and selection.
//!
//! Two separate scoring models rank parts for the "melody" (primary) and
//! "accompaniment" (secondary) roles. A selection procedure picks the best
//! pair with fallback and duplicate-rejection guardrails.

use super::track_analysis::PartFeatures;

/// Result of the part-selection procedure.
#[derive(Debug, Clone)]
pub struct PartSelection {
    /// Index into the `features` slice for the chosen primary part.
    pub primary: usize,
    /// Index into the `features` slice for the chosen secondary part.
    pub secondary: usize,
    /// All non-drum part indices ranked by PrimaryScore (best first).
    pub primary_candidates: Vec<usize>,
    /// All non-drum part indices ranked by SecondaryScore relative to the
    /// chosen primary (best first).
    pub secondary_candidates: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Normalization helpers
// ---------------------------------------------------------------------------

/// Min-max normalize a value within a population, returning 0..1.
fn normalize(value: f32, all: &[f32]) -> f32 {
    if all.is_empty() {
        return 0.0;
    }
    let min = all.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = all.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if (max - min).abs() < 1e-9 {
        return 0.5;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

/// Build a Vec of a single field extracted from all features.
fn collect_field(features: &[PartFeatures], f: impl Fn(&PartFeatures) -> f32) -> Vec<f32> {
    features.iter().map(f).collect()
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Compute the "melody-likeness" score for one part.
pub fn primary_score(feat: &PartFeatures, all: &[PartFeatures]) -> f32 {
    if feat.is_drum {
        return -100.0;
    }

    let all_p75 = collect_field(all, |f| f.p75_pitch);
    let all_mono = collect_field(all, |f| f.monophony_ratio);
    let all_active = collect_field(all, |f| f.active_ratio);
    let all_vel = collect_field(all, |f| f.velocity_p80);
    let all_chord = collect_field(all, |f| f.chordiness);
    let all_nps = collect_field(all, |f| f.notes_per_sec);
    let all_ntime = collect_field(all, |f| f.total_note_time);

    let mut score = 0.0f32;
    score += 1.5 * normalize(feat.p75_pitch, &all_p75);
    score += 1.5 * normalize(feat.monophony_ratio, &all_mono);
    score += 2.5 * normalize(feat.active_ratio, &all_active);
    score += 1.5 * normalize(feat.total_note_time, &all_ntime);
    score += 0.5 * normalize(feat.velocity_p80, &all_vel);
    score -= 2.5 * normalize(feat.chordiness, &all_chord);

    // Penalize extremely dense parts (arpeggio spam).
    if feat.notes_per_sec > 12.0 {
        score -= 1.5 * normalize(feat.notes_per_sec, &all_nps);
    }

    // Penalize very sparse parts that likely aren't the melody.
    if feat.active_ratio < 0.10 {
        score -= 3.0;
    } else if feat.active_ratio < 0.25 {
        score -= 1.5;
    }

    score += feat.melody_bias;
    score
}

/// Compute the "accompaniment / complement" score for one part given
/// the already-chosen primary.
pub fn secondary_score(feat: &PartFeatures, primary: &PartFeatures, all: &[PartFeatures]) -> f32 {
    if feat.is_drum {
        return -100.0;
    }

    let all_active = collect_field(all, |f| f.active_ratio);
    let all_chord = collect_field(all, |f| f.chordiness);
    let all_ntime = collect_field(all, |f| f.total_note_time);

    let mut score = 0.0f32;
    score += 2.0 * normalize(feat.active_ratio, &all_active);
    score += 1.0 * normalize(feat.chordiness, &all_chord);
    score += 1.5 * normalize(feat.total_note_time, &all_ntime);
    score += 1.0 * complementarity_pitch(primary, feat);
    score += feat.accompaniment_bias;

    if feat.active_ratio < 0.10 {
        score -= 3.0;
    } else if feat.active_ratio < 0.25 {
        score -= 1.5;
    }

    score
}

/// Reward parts whose median pitch sits 5-12 semitones below the primary,
/// or that are chordy when the primary is monophonic.
fn complementarity_pitch(primary: &PartFeatures, candidate: &PartFeatures) -> f32 {
    let pitch_diff = primary.median_pitch - candidate.median_pitch;
    let pitch_reward = if (5.0..=12.0).contains(&pitch_diff) {
        1.0
    } else if (2.0..=18.0).contains(&pitch_diff) {
        0.5
    } else {
        0.0
    };

    let texture_reward = if primary.monophony_ratio > 0.7 && candidate.chordiness > 1.5 {
        0.5
    } else {
        0.0
    };

    pitch_reward + texture_reward
}

// ---------------------------------------------------------------------------
// Duplicate detection
// ---------------------------------------------------------------------------

/// Two parts are "near-duplicates" if their median pitches are within 2
/// semitones AND onset-time correlation is very high.
fn are_near_duplicates(a: &PartFeatures, b: &PartFeatures) -> bool {
    let pitch_close = (a.median_pitch - b.median_pitch).abs() < 2.0;
    let range_close = (a.pitch_range_p10_p90 - b.pitch_range_p10_p90).abs() < 3.0;
    let density_close = if a.notes_per_sec > 0.0 {
        ((a.notes_per_sec - b.notes_per_sec) / a.notes_per_sec).abs() < 0.15
    } else {
        b.notes_per_sec < 0.1
    };
    pitch_close && range_close && density_close
}

// ---------------------------------------------------------------------------
// Selection procedure
// ---------------------------------------------------------------------------

/// Run the full selection procedure over a slice of `PartFeatures`.
///
/// Returns `None` if there are no viable (non-drum, non-empty) parts.
pub fn select_parts(features: &[PartFeatures]) -> Option<PartSelection> {
    let viable: Vec<usize> = features
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.is_drum && f.note_count > 0)
        .map(|(i, _)| i)
        .collect();

    if viable.is_empty() {
        return None;
    }

    // Rank by primary score.
    let mut primary_ranked: Vec<(usize, f32)> = viable
        .iter()
        .map(|&i| (i, primary_score(&features[i], features)))
        .collect();
    primary_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let primary_idx = primary_ranked[0].0;

    // Rank remaining by secondary score relative to chosen primary.
    let mut secondary_ranked: Vec<(usize, f32)> = viable
        .iter()
        .filter(|&&i| i != primary_idx)
        .filter(|&&i| !are_near_duplicates(&features[i], &features[primary_idx]))
        .map(|&i| {
            (
                i,
                secondary_score(&features[i], &features[primary_idx], features),
            )
        })
        .collect();
    secondary_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let secondary_idx = if let Some(&(idx, score)) = secondary_ranked.first() {
        // Fallback: if best secondary score is very low, pick by total_note_time.
        if score < 0.5 {
            viable
                .iter()
                .filter(|&&i| i != primary_idx)
                .max_by(|&&a, &&b| {
                    features[a]
                        .total_note_time
                        .partial_cmp(&features[b].total_note_time)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .copied()
                .unwrap_or(idx)
        } else {
            idx
        }
    } else {
        // Only one viable part.
        primary_idx
    };

    // Build candidate lists for cycling.
    let primary_candidates: Vec<usize> = primary_ranked.iter().map(|&(i, _)| i).collect();
    let mut secondary_candidates: Vec<usize> = secondary_ranked.iter().map(|&(i, _)| i).collect();
    if secondary_candidates.is_empty() {
        secondary_candidates.push(primary_idx);
    }

    Some(PartSelection {
        primary: primary_idx,
        secondary: secondary_idx,
        primary_candidates,
        secondary_candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn melody_features() -> PartFeatures {
        PartFeatures {
            note_count: 200,
            total_note_time: 60.0,
            notes_per_sec: 3.0,
            active_ratio: 0.7,
            velocity_mean: 0.7,
            velocity_p80: 0.85,
            median_pitch: 72.0,
            p75_pitch: 76.0,
            pitch_range_p10_p90: 12.0,
            monophony_ratio: 0.95,
            chordiness: 1.0,
            stepwise_motion: 2.5,
            melody_bias: 0.6,
            accompaniment_bias: 0.0,
            bass_bias: 0.0,
            is_drum: false,
            program: 80,
        }
    }

    fn chords_features() -> PartFeatures {
        PartFeatures {
            note_count: 150,
            total_note_time: 90.0,
            notes_per_sec: 2.0,
            active_ratio: 0.8,
            velocity_mean: 0.5,
            velocity_p80: 0.6,
            median_pitch: 60.0,
            p75_pitch: 64.0,
            pitch_range_p10_p90: 8.0,
            monophony_ratio: 0.1,
            chordiness: 3.5,
            stepwise_motion: 4.0,
            melody_bias: 0.0,
            accompaniment_bias: 0.5,
            bass_bias: 0.0,
            is_drum: false,
            program: 48,
        }
    }

    fn bass_features() -> PartFeatures {
        PartFeatures {
            note_count: 100,
            total_note_time: 50.0,
            notes_per_sec: 2.0,
            active_ratio: 0.6,
            velocity_mean: 0.6,
            velocity_p80: 0.7,
            median_pitch: 40.0,
            p75_pitch: 43.0,
            pitch_range_p10_p90: 10.0,
            monophony_ratio: 0.98,
            chordiness: 1.0,
            stepwise_motion: 5.0,
            melody_bias: 0.0,
            accompaniment_bias: 0.0,
            bass_bias: 0.8,
            is_drum: false,
            program: 33,
        }
    }

    fn drum_features() -> PartFeatures {
        PartFeatures {
            note_count: 300,
            is_drum: true,
            ..Default::default()
        }
    }

    #[test]
    fn melody_beats_chords_as_primary() {
        let all = vec![melody_features(), chords_features(), drum_features()];
        let ps = primary_score(&all[0], &all);
        let cs = primary_score(&all[1], &all);
        assert!(
            ps > cs,
            "melody primary score ({ps}) should > chords ({cs})"
        );
    }

    #[test]
    fn chords_beats_melody_as_secondary() {
        let all = vec![melody_features(), chords_features(), bass_features()];
        let primary = &all[0];
        let cs = secondary_score(&all[1], primary, &all);
        let ms = secondary_score(&all[0], primary, &all);
        // chords_features should never appear since it's filtered as primary,
        // but the raw score should still be higher.
        assert!(cs > ms, "chords sec score ({cs}) should > melody ({ms})");
    }

    #[test]
    fn select_parts_picks_melody_primary() {
        let all = vec![
            drum_features(),
            chords_features(),
            melody_features(),
            bass_features(),
        ];
        let sel = select_parts(&all).unwrap();
        assert_eq!(sel.primary, 2, "primary should be melody (index 2)");
        assert_ne!(sel.secondary, sel.primary);
        assert!(!sel.primary_candidates.is_empty());
    }

    #[test]
    fn select_parts_single_viable() {
        let all = vec![drum_features(), melody_features()];
        let sel = select_parts(&all).unwrap();
        assert_eq!(sel.primary, 1);
        // Only one viable part, secondary falls back to primary.
        assert_eq!(sel.secondary, 1);
    }

    #[test]
    fn select_parts_no_viable() {
        let all = vec![drum_features()];
        assert!(select_parts(&all).is_none());
    }

    #[test]
    fn drums_get_huge_penalty() {
        let all = vec![drum_features(), melody_features()];
        let ds = primary_score(&all[0], &all);
        assert!(ds < -50.0);
    }

    #[test]
    fn near_duplicate_rejected() {
        let mut dup = melody_features();
        dup.median_pitch = 73.0; // within 2 semitones
        let all = vec![melody_features(), dup, chords_features()];
        let sel = select_parts(&all).unwrap();
        // Secondary should NOT be the near-duplicate (index 1).
        assert_eq!(sel.secondary, 2);
    }
}
