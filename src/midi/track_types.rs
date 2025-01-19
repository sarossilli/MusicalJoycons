#[derive(Debug, Clone)]  // Add Clone here
pub struct TrackMetrics {
    pub track_index: usize,
    pub note_count: usize,
    pub unique_notes: usize,
    pub avg_velocity: f32,
    pub velocity_variance: f32,
    pub avg_note_duration: f32,
    pub total_duration: f32,
    pub note_density: f32,
    pub is_percussion: bool,
    pub track_name: Option<String>,
    pub track_instrument: Option<String>,
    pub track_type: TrackType,
    pub pitch_range: u8,
    pub melodic_movement: f32,
    pub sustain_ratio: f32,
    pub rhythmic_regularity: f32,
}

#[derive(Debug, PartialEq, Clone)]  // Add Clone here
pub enum TrackType {
    Melody,
    Harmony,
    Bass,
    Drums,
    Vocals,
    Unknown,
}

pub(crate) struct TrackWeights {
    pub note_density: f32,
    pub velocity_variance: f32,
    pub unique_notes: f32,
    pub avg_note_duration: f32,
    pub pitch_range: f32,
    pub melodic_movement: f32,
    pub sustain_ratio: f32,
    pub rhythmic_regularity: f32,
}

impl Default for TrackMetrics {
    fn default() -> Self {
        Self {
            track_index: 0,
            note_count: 0,
            unique_notes: 0,
            avg_velocity: 0.0,
            velocity_variance: 0.0,
            avg_note_duration: 0.0,
            total_duration: 0.0,
            note_density: 0.0,
            is_percussion: false,
            track_name: None,
            track_instrument: None,
            track_type: TrackType::Unknown,
            pitch_range: 0,
            melodic_movement: 0.0,
            sustain_ratio: 0.0,
            rhythmic_regularity: 0.0,
        }
    }
}

impl TrackMetrics {
    pub fn calculate_score(&self) -> f32 {
        if self.is_percussion {
            return 0.0;
        }

        let weights = TrackWeights {
            note_density: 0.25,
            velocity_variance: 0.10,
            unique_notes: 0.15,
            avg_note_duration: 0.15,
            pitch_range: 0.15,
            melodic_movement: 0.10,
            sustain_ratio: 0.05,
            rhythmic_regularity: 0.05,
        };


        let density_score = {
            let ideal_density = 3.0;
            let density_diff = (self.note_density - ideal_density).abs();
            (1.0 - (density_diff / 5.0)).max(0.0) * weights.note_density
        };

        let velocity_score = (self.velocity_variance / 30.0).min(1.0) * weights.velocity_variance;

        let variety_score = {
            let unique_ratio = self.unique_notes as f32 / self.note_count.max(1) as f32;
            let ideal_ratio = 0.5;
            (1.0 - (unique_ratio - ideal_ratio).abs()) * weights.unique_notes
        };

        let duration_score = {
            let ideal_duration = 0.3;
            let duration_diff = (self.avg_note_duration - ideal_duration).abs();
            (1.0 - (duration_diff / 1.0)).max(0.0) * weights.avg_note_duration
        };

        let pitch_range_score = {
            let normalized_range = self.pitch_range as f32 / 127.0;
            let ideal_range = 0.3;
            (1.0 - (normalized_range - ideal_range).abs()) * weights.pitch_range
        };

        let melodic_movement_score = {
            let ideal_movement = 2.0;
            let movement_diff = (self.melodic_movement - ideal_movement).abs();
            (1.0 - (movement_diff / 12.0)).max(0.0) * weights.melodic_movement
        };

        let sustain_score = {
            let ideal_sustain = 0.6;
            (1.0 - (self.sustain_ratio - ideal_sustain).abs()) * weights.sustain_ratio
        };

        let rhythmic_score = self.rhythmic_regularity * weights.rhythmic_regularity;

        let base_score = density_score
            + velocity_score
            + variety_score
            + duration_score
            + pitch_range_score
            + melodic_movement_score
            + sustain_score
            + rhythmic_score;

        let type_multiplier = match self.track_type {
            TrackType::Harmony => 1.5,
            TrackType::Melody => 1.3,
            TrackType::Bass => 1.1,
            TrackType::Vocals => 1.1,
            TrackType::Drums => 0.2,
            TrackType::Unknown => 1.0,
        };

        let bonus_multiplier = if self.note_density >= 2.0 && self.note_density <= 4.0
            && self.unique_notes >= 5 && self.unique_notes <= 20
            && self.note_count >= 300
        {
            1.3
        } else {
            1.0
        };

        base_score * type_multiplier * bonus_multiplier
    }

    // Add new method to calculate score for a specific time window
    pub fn calculate_window_score(&self, window_note_count: usize, window_duration: f32) -> f32 {
        if self.is_percussion {
            return 0.0;
        }

        // Make note count the primary factor
        let note_score = window_note_count as f32 * 0.8; // 80% of score comes from note count

        // Calculate density as a bonus multiplier
        let window_density = if window_duration > 0.0 {
            window_note_count as f32 / window_duration
        } else {
            0.0
        };

        // Small bonus for good density (avoid extremely sparse or dense sections)
        let density_multiplier = {
            let ideal_density = 3.0;
            let density_diff = (window_density - ideal_density).abs();
            1.0 + (0.2 * (1.0 - (density_diff / 5.0)).max(0.0)) // Up to 20% bonus
        };

        // Apply track type as a smaller multiplier
        let type_multiplier = match self.track_type {
            TrackType::Melody => 1.15,  // Reduced from 1.3
            TrackType::Bass => 1.1,     // Same
            TrackType::Harmony => 1.05,  // Reduced from 1.5
            TrackType::Vocals => 1.1,    // Same
            TrackType::Drums => 0.0,
            TrackType::Unknown => 1.0,
        };

        note_score * density_multiplier * type_multiplier
    }

    pub fn determine_track_type(&mut self, program_number: Option<u8>) {
        if self.is_percussion {
            self.track_type = TrackType::Drums;
            return;
        }

        // First check program numbers if available
        if let Some(program) = program_number {
            self.track_type = match program {
                0..=7 => TrackType::Melody,      // Piano family
                8..=15 => TrackType::Harmony,    // Chromatic Percussion
                16..=23 => TrackType::Harmony,   // Organ
                24..=31 => TrackType::Harmony,   // Guitar
                32..=39 => TrackType::Bass,      // Bass
                40..=47 => TrackType::Harmony,   // Strings
                48..=55 => TrackType::Harmony,   // Ensemble
                56..=63 => TrackType::Melody,    // Brass
                64..=71 => TrackType::Melody,    // Reed
                72..=79 => TrackType::Melody,    // Pipe
                80..=87 => TrackType::Melody,    // Synth Lead
                88..=95 => TrackType::Harmony,   // Synth Pad
                96..=103 => TrackType::Harmony,  // Synth Effects
                104..=111 => TrackType::Melody,  // Ethnic
                112..=119 => TrackType::Drums,   // Percussive
                120..=127 => TrackType::Unknown, // Sound Effects
                _ => TrackType::Unknown,
            };
            return;
        }

        // Check track name and instrument hints
        let track_name = self.track_name.as_deref().unwrap_or("").to_lowercase();
        let instrument = self
            .track_instrument
            .as_deref()
            .unwrap_or("")
            .to_lowercase();

        let is_likely_vocal = track_name.contains("voc")
            || track_name.contains("sing")
            || track_name.contains("voice")
            || instrument.contains("vocal");

        let is_likely_bass = track_name.contains("bass") || instrument.contains("bass");

        let is_likely_melody = track_name.contains("lead")
            || track_name.contains("melody")
            || instrument.contains("lead");

        // Determine type based on characteristics if no clear naming matches
        if is_likely_vocal {
            self.track_type = TrackType::Vocals;
        } else if is_likely_bass {
            self.track_type = TrackType::Bass;
        } else if is_likely_melody {
            self.track_type = TrackType::Melody;
        } else if self.note_density > 3.0 && self.unique_notes > 12 {
            self.track_type = TrackType::Melody;
        } else if self.sustain_ratio > 0.7 {
            self.track_type = TrackType::Harmony;
        } else {
            self.track_type = TrackType::Unknown;
        }
    }
}
