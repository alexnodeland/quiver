//! Preset Library
//!
//! This module provides a collection of ready-to-use patch presets:
//! - Classic synth patches (bass, lead, pad, etc.)
//! - Sound design examples
//! - Tutorial patches for learning

use crate::serialize::{CableDef, ModuleDef, PatchDef};

/// Preset category for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetCategory {
    /// Classic synthesizer sounds
    Classic,
    /// Sound design and experimental
    SoundDesign,
    /// Educational/tutorial patches
    Tutorial,
    /// Bass sounds
    Bass,
    /// Lead sounds
    Lead,
    /// Pad/ambient sounds
    Pad,
    /// Percussion/drums
    Percussion,
    /// Effects and processing
    Effect,
}

/// Preset metadata
#[derive(Debug, Clone)]
pub struct PresetInfo {
    /// Preset name
    pub name: String,
    /// Category
    pub category: PresetCategory,
    /// Description
    pub description: String,
    /// Tags for searching
    pub tags: Vec<String>,
    /// Difficulty level (1-5, for tutorials)
    pub difficulty: Option<u8>,
}

impl PresetInfo {
    pub fn new(name: impl Into<String>, category: PresetCategory) -> Self {
        Self {
            name: name.into(),
            category,
            description: String::new(),
            tags: Vec::new(),
            difficulty: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_difficulty(mut self, level: u8) -> Self {
        self.difficulty = Some(level.min(5));
        self
    }
}

/// Preset library containing all available presets
pub struct PresetLibrary;

impl PresetLibrary {
    /// Get all available preset infos
    pub fn list() -> Vec<PresetInfo> {
        vec![
            // Classic patches
            PresetInfo::new("Moog Bass", PresetCategory::Bass)
                .with_description("Classic Moog-style monophonic bass")
                .with_tag("analog")
                .with_tag("mono")
                .with_tag("fat"),
            PresetInfo::new("303 Acid", PresetCategory::Bass)
                .with_description("Roland TB-303 style acid bass")
                .with_tag("acid")
                .with_tag("resonant")
                .with_tag("squelchy"),
            PresetInfo::new("Juno Pad", PresetCategory::Pad)
                .with_description("Warm Roland Juno-style pad")
                .with_tag("analog")
                .with_tag("warm")
                .with_tag("lush"),
            PresetInfo::new("Sync Lead", PresetCategory::Lead)
                .with_description("Aggressive sync lead sound")
                .with_tag("sync")
                .with_tag("aggressive")
                .with_tag("bright"),
            PresetInfo::new("PWM Strings", PresetCategory::Pad)
                .with_description("Pulse width modulated string-like pad")
                .with_tag("pwm")
                .with_tag("strings")
                .with_tag("ensemble"),
            // Sound design
            PresetInfo::new("Metallic Ring", PresetCategory::SoundDesign)
                .with_description("Ring modulation metallic texture")
                .with_tag("ring-mod")
                .with_tag("metallic")
                .with_tag("experimental"),
            PresetInfo::new("Noise Sweep", PresetCategory::SoundDesign)
                .with_description("Filtered noise with resonant sweep")
                .with_tag("noise")
                .with_tag("sweep")
                .with_tag("fx"),
            PresetInfo::new("Wavefold Growl", PresetCategory::SoundDesign)
                .with_description("Aggressive wavefolding distortion")
                .with_tag("wavefolder")
                .with_tag("distortion")
                .with_tag("aggressive"),
            // Tutorial patches
            PresetInfo::new("Basic Subtractive", PresetCategory::Tutorial)
                .with_description("Simple VCO -> VCF -> VCA patch")
                .with_tag("beginner")
                .with_tag("subtractive")
                .with_difficulty(1),
            PresetInfo::new("Envelope Basics", PresetCategory::Tutorial)
                .with_description("Learn ADSR envelope shaping")
                .with_tag("beginner")
                .with_tag("envelope")
                .with_difficulty(1),
            PresetInfo::new("Filter Modulation", PresetCategory::Tutorial)
                .with_description("LFO modulating filter cutoff")
                .with_tag("beginner")
                .with_tag("modulation")
                .with_difficulty(2),
            PresetInfo::new("FM Basics", PresetCategory::Tutorial)
                .with_description("Introduction to FM synthesis")
                .with_tag("intermediate")
                .with_tag("fm")
                .with_difficulty(3),
        ]
    }

    /// Get presets by category
    pub fn by_category(category: PresetCategory) -> Vec<PresetInfo> {
        Self::list()
            .into_iter()
            .filter(|p| p.category == category)
            .collect()
    }

    /// Search presets by tag
    pub fn by_tag(tag: &str) -> Vec<PresetInfo> {
        let tag_lower = tag.to_lowercase();
        Self::list()
            .into_iter()
            .filter(|p| p.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower)))
            .collect()
    }

    /// Load a preset by name
    pub fn load(name: &str) -> Option<PatchDef> {
        match name {
            // Classic patches
            "Moog Bass" => Some(ClassicPresets::moog_bass()),
            "303 Acid" => Some(ClassicPresets::acid_303()),
            "Juno Pad" => Some(ClassicPresets::juno_pad()),
            "Sync Lead" => Some(ClassicPresets::sync_lead()),
            "PWM Strings" => Some(ClassicPresets::pwm_strings()),
            // Sound design
            "Metallic Ring" => Some(SoundDesignPresets::metallic_ring()),
            "Noise Sweep" => Some(SoundDesignPresets::noise_sweep()),
            "Wavefold Growl" => Some(SoundDesignPresets::wavefold_growl()),
            // Tutorials
            "Basic Subtractive" => Some(TutorialPresets::basic_subtractive()),
            "Envelope Basics" => Some(TutorialPresets::envelope_basics()),
            "Filter Modulation" => Some(TutorialPresets::filter_modulation()),
            "FM Basics" => Some(TutorialPresets::fm_basics()),
            _ => None,
        }
    }
}

// =============================================================================
// Classic Synth Presets
// =============================================================================

/// Classic synthesizer patch presets
pub struct ClassicPresets;

impl ClassicPresets {
    /// Moog-style monophonic bass
    ///
    /// Classic fat bass sound using:
    /// - Two detuned oscillators (saw waves)
    /// - Low-pass filter with envelope
    /// - VCA with envelope
    pub fn moog_bass() -> PatchDef {
        let mut patch = PatchDef::new("Moog Bass")
            .with_author("Quiver")
            .with_description("Classic Moog-style monophonic bass with two detuned oscillators")
            .with_tag("bass")
            .with_tag("analog")
            .with_tag("classic");

        // Modules
        patch.modules = vec![
            ModuleDef::new("vco1", "vco").with_position(100.0, 100.0),
            ModuleDef::new("vco2", "vco").with_position(100.0, 200.0),
            ModuleDef::new("mixer", "mixer").with_position(250.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(400.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(550.0, 150.0),
            ModuleDef::new("env_filter", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("env_amp", "adsr").with_position(550.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(700.0, 150.0),
        ];

        // Cables
        patch.cables = vec![
            // VCO1 saw -> mixer ch1
            CableDef::new("vco1.saw", "mixer.in1"),
            // VCO2 saw -> mixer ch2 (slightly detuned via offset)
            CableDef::new("vco2.saw", "mixer.in2"),
            // Mixer -> filter
            CableDef::new("mixer.out", "vcf.in"),
            // Filter LP -> VCA
            CableDef::new("vcf.lp", "vca.in"),
            // VCA -> output
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            // Filter envelope -> cutoff (with attenuation)
            CableDef::new("env_filter.out", "vcf.cutoff").with_attenuation(0.6),
            // Amp envelope -> VCA
            CableDef::new("env_amp.out", "vca.cv"),
        ];

        // Parameters
        patch.parameters.insert("vcf.cutoff".into(), 0.3);
        patch.parameters.insert("vcf.resonance".into(), 0.4);
        patch.parameters.insert("env_filter.attack".into(), 0.01);
        patch.parameters.insert("env_filter.decay".into(), 0.3);
        patch.parameters.insert("env_filter.sustain".into(), 0.2);
        patch.parameters.insert("env_filter.release".into(), 0.2);
        patch.parameters.insert("env_amp.attack".into(), 0.01);
        patch.parameters.insert("env_amp.decay".into(), 0.1);
        patch.parameters.insert("env_amp.sustain".into(), 0.8);
        patch.parameters.insert("env_amp.release".into(), 0.3);

        patch
    }

    /// TB-303 style acid bass
    ///
    /// Squelchy resonant bass using:
    /// - Single square wave oscillator
    /// - Highly resonant low-pass filter
    /// - Accent via envelope depth
    pub fn acid_303() -> PatchDef {
        let mut patch = PatchDef::new("303 Acid")
            .with_author("Quiver")
            .with_description("Roland TB-303 style acid bass with squelchy resonance")
            .with_tag("bass")
            .with_tag("acid")
            .with_tag("303");

        patch.modules = vec![
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("vcf", "diode_ladder").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("env", "adsr").with_position(250.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco.sqr", "vcf.in"),
            CableDef::new("vcf.out", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vcf.cutoff").with_attenuation(0.8),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.2);
        patch.parameters.insert("vcf.resonance".into(), 0.85);
        patch.parameters.insert("env.attack".into(), 0.001);
        patch.parameters.insert("env.decay".into(), 0.2);
        patch.parameters.insert("env.sustain".into(), 0.0);
        patch.parameters.insert("env.release".into(), 0.1);

        patch
    }

    /// Juno-style warm pad
    ///
    /// Lush pad sound using:
    /// - PWM oscillator with slow LFO
    /// - Gentle filtering
    /// - Slow attack envelope
    pub fn juno_pad() -> PatchDef {
        let mut patch = PatchDef::new("Juno Pad")
            .with_author("Quiver")
            .with_description("Warm Roland Juno-style pad with PWM")
            .with_tag("pad")
            .with_tag("analog")
            .with_tag("warm");

        patch.modules = vec![
            ModuleDef::new("lfo", "lfo").with_position(100.0, 50.0),
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("env", "adsr").with_position(250.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            // LFO -> pulse width for PWM
            CableDef::new("lfo.tri", "vco.pw")
                .with_attenuation(0.3)
                .with_offset(0.5),
            // Square wave (PWM) -> filter
            CableDef::new("vco.sqr", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("lfo.rate".into(), 0.2);
        patch.parameters.insert("vcf.cutoff".into(), 0.6);
        patch.parameters.insert("vcf.resonance".into(), 0.1);
        patch.parameters.insert("env.attack".into(), 0.5);
        patch.parameters.insert("env.decay".into(), 0.3);
        patch.parameters.insert("env.sustain".into(), 0.7);
        patch.parameters.insert("env.release".into(), 1.0);

        patch
    }

    /// Hard sync lead
    ///
    /// Aggressive lead using oscillator sync:
    /// - Master and slave oscillators
    /// - Slave frequency swept by envelope
    /// - Bright, cutting sound
    pub fn sync_lead() -> PatchDef {
        let mut patch = PatchDef::new("Sync Lead")
            .with_author("Quiver")
            .with_description("Aggressive oscillator sync lead sound")
            .with_tag("lead")
            .with_tag("sync")
            .with_tag("bright");

        patch.modules = vec![
            ModuleDef::new("vco_master", "vco").with_position(100.0, 100.0),
            ModuleDef::new("vco_slave", "vco").with_position(100.0, 200.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("env_sync", "adsr").with_position(100.0, 350.0),
            ModuleDef::new("env_amp", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            // Master sync output to slave
            CableDef::new("vco_master.sqr", "vco_slave.sync"),
            // Slave saw -> filter (the synced output)
            CableDef::new("vco_slave.saw", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            // Envelope sweeps slave pitch for sync sweep
            CableDef::new("env_sync.out", "vco_slave.fm").with_attenuation(0.5),
            CableDef::new("env_amp.out", "vca.cv"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.7);
        patch.parameters.insert("vcf.resonance".into(), 0.2);
        patch.parameters.insert("env_sync.attack".into(), 0.01);
        patch.parameters.insert("env_sync.decay".into(), 0.4);
        patch.parameters.insert("env_sync.sustain".into(), 0.3);
        patch.parameters.insert("env_sync.release".into(), 0.2);
        patch.parameters.insert("env_amp.attack".into(), 0.01);
        patch.parameters.insert("env_amp.decay".into(), 0.1);
        patch.parameters.insert("env_amp.sustain".into(), 0.8);
        patch.parameters.insert("env_amp.release".into(), 0.3);

        patch
    }

    /// PWM string ensemble
    ///
    /// String-like pad using:
    /// - Multiple PWM oscillators
    /// - Chorus-like detuning
    /// - Slow attack for bowed effect
    pub fn pwm_strings() -> PatchDef {
        let mut patch = PatchDef::new("PWM Strings")
            .with_author("Quiver")
            .with_description("Lush PWM string ensemble sound")
            .with_tag("pad")
            .with_tag("strings")
            .with_tag("ensemble");

        patch.modules = vec![
            ModuleDef::new("lfo1", "lfo").with_position(50.0, 50.0),
            ModuleDef::new("lfo2", "lfo").with_position(150.0, 50.0),
            ModuleDef::new("vco1", "vco").with_position(100.0, 150.0),
            ModuleDef::new("vco2", "vco").with_position(100.0, 250.0),
            ModuleDef::new("mixer", "mixer").with_position(250.0, 200.0),
            ModuleDef::new("vcf", "svf").with_position(400.0, 200.0),
            ModuleDef::new("vca", "vca").with_position(550.0, 200.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 350.0),
            ModuleDef::new("output", "stereo_output").with_position(700.0, 200.0),
        ];

        patch.cables = vec![
            // LFOs modulate pulse widths at different rates
            CableDef::new("lfo1.tri", "vco1.pw")
                .with_attenuation(0.25)
                .with_offset(0.5),
            CableDef::new("lfo2.tri", "vco2.pw")
                .with_attenuation(0.25)
                .with_offset(0.5),
            // Mix oscillators
            CableDef::new("vco1.sqr", "mixer.in1"),
            CableDef::new("vco2.sqr", "mixer.in2"),
            CableDef::new("mixer.out", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("lfo1.rate".into(), 0.15);
        patch.parameters.insert("lfo2.rate".into(), 0.22);
        patch.parameters.insert("vcf.cutoff".into(), 0.5);
        patch.parameters.insert("vcf.resonance".into(), 0.05);
        patch.parameters.insert("env.attack".into(), 0.8);
        patch.parameters.insert("env.decay".into(), 0.2);
        patch.parameters.insert("env.sustain".into(), 0.9);
        patch.parameters.insert("env.release".into(), 1.5);

        patch
    }
}

// =============================================================================
// Sound Design Presets
// =============================================================================

/// Sound design and experimental presets
pub struct SoundDesignPresets;

impl SoundDesignPresets {
    /// Metallic ring modulation texture
    pub fn metallic_ring() -> PatchDef {
        let mut patch = PatchDef::new("Metallic Ring")
            .with_author("Quiver")
            .with_description("Ring modulation creating metallic, bell-like textures")
            .with_tag("ring-mod")
            .with_tag("metallic")
            .with_tag("experimental");

        patch.modules = vec![
            ModuleDef::new("vco1", "vco").with_position(100.0, 100.0),
            ModuleDef::new("vco2", "vco").with_position(100.0, 200.0),
            ModuleDef::new("ring", "ring_modulator").with_position(250.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(400.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(550.0, 150.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(700.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco1.sin", "ring.carrier"),
            CableDef::new("vco2.sin", "ring.modulator"),
            CableDef::new("ring.out", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.8);
        patch.parameters.insert("vcf.resonance".into(), 0.1);
        patch.parameters.insert("env.attack".into(), 0.01);
        patch.parameters.insert("env.decay".into(), 1.0);
        patch.parameters.insert("env.sustain".into(), 0.3);
        patch.parameters.insert("env.release".into(), 0.5);

        patch
    }

    /// Filtered noise sweep
    pub fn noise_sweep() -> PatchDef {
        let mut patch = PatchDef::new("Noise Sweep")
            .with_author("Quiver")
            .with_description("Resonant filter sweep on noise for FX and transitions")
            .with_tag("noise")
            .with_tag("sweep")
            .with_tag("fx");

        patch.modules = vec![
            ModuleDef::new("noise", "noise_generator").with_position(100.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("lfo", "lfo").with_position(250.0, 300.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("noise.white", "vcf.in"),
            CableDef::new("vcf.bp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("lfo.tri", "vcf.cutoff").with_attenuation(0.4),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("lfo.rate".into(), 0.1);
        patch.parameters.insert("vcf.cutoff".into(), 0.5);
        patch.parameters.insert("vcf.resonance".into(), 0.8);
        patch.parameters.insert("env.attack".into(), 0.5);
        patch.parameters.insert("env.decay".into(), 0.0);
        patch.parameters.insert("env.sustain".into(), 1.0);
        patch.parameters.insert("env.release".into(), 0.5);

        patch
    }

    /// Wavefolding distortion
    pub fn wavefold_growl() -> PatchDef {
        let mut patch = PatchDef::new("Wavefold Growl")
            .with_author("Quiver")
            .with_description("Aggressive wavefolding distortion for bass and leads")
            .with_tag("wavefolder")
            .with_tag("distortion")
            .with_tag("aggressive");

        patch.modules = vec![
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("folder", "wavefolder").with_position(250.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(400.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(550.0, 150.0),
            ModuleDef::new("lfo", "lfo").with_position(250.0, 300.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(700.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco.sin", "folder.in"),
            CableDef::new("folder.out", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            // LFO modulates fold amount
            CableDef::new("lfo.tri", "folder.amount").with_attenuation(0.3),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("lfo.rate".into(), 0.3);
        patch.parameters.insert("folder.amount".into(), 0.7);
        patch.parameters.insert("vcf.cutoff".into(), 0.6);
        patch.parameters.insert("vcf.resonance".into(), 0.3);
        patch.parameters.insert("env.attack".into(), 0.01);
        patch.parameters.insert("env.decay".into(), 0.2);
        patch.parameters.insert("env.sustain".into(), 0.7);
        patch.parameters.insert("env.release".into(), 0.3);

        patch
    }
}

// =============================================================================
// Tutorial Presets
// =============================================================================

/// Educational tutorial presets
pub struct TutorialPresets;

impl TutorialPresets {
    /// Basic subtractive synthesis
    ///
    /// The simplest subtractive synth patch:
    /// VCO -> VCF -> VCA -> Output
    pub fn basic_subtractive() -> PatchDef {
        let mut patch = PatchDef::new("Basic Subtractive")
            .with_author("Quiver")
            .with_description(
                "Tutorial: Basic subtractive synthesis chain. \
                 VCO generates the raw waveform, VCF shapes the timbre, \
                 VCA controls the volume.",
            )
            .with_tag("tutorial")
            .with_tag("beginner");

        patch.modules = vec![
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco.saw", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.5);
        patch.parameters.insert("vcf.resonance".into(), 0.2);
        patch.parameters.insert("vca.level".into(), 0.7);

        patch
    }

    /// Envelope basics
    ///
    /// Shows how ADSR envelope shapes the sound:
    /// VCO -> VCF -> VCA (with envelope)
    pub fn envelope_basics() -> PatchDef {
        let mut patch = PatchDef::new("Envelope Basics")
            .with_author("Quiver")
            .with_description(
                "Tutorial: ADSR envelope controlling VCA. \
                 Attack = fade in time, Decay = drop to sustain, \
                 Sustain = held level, Release = fade out after gate off.",
            )
            .with_tag("tutorial")
            .with_tag("beginner")
            .with_tag("envelope");

        patch.modules = vec![
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco.saw", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.6);
        patch.parameters.insert("vcf.resonance".into(), 0.1);
        patch.parameters.insert("env.attack".into(), 0.1);
        patch.parameters.insert("env.decay".into(), 0.3);
        patch.parameters.insert("env.sustain".into(), 0.5);
        patch.parameters.insert("env.release".into(), 0.4);

        patch
    }

    /// Filter modulation with LFO
    ///
    /// LFO modulating filter cutoff for wah-wah effect
    pub fn filter_modulation() -> PatchDef {
        let mut patch = PatchDef::new("Filter Modulation")
            .with_author("Quiver")
            .with_description(
                "Tutorial: LFO modulating filter cutoff. \
                 The LFO (Low Frequency Oscillator) creates a repeating \
                 sweep of the filter, creating a 'wah-wah' effect.",
            )
            .with_tag("tutorial")
            .with_tag("beginner")
            .with_tag("modulation");

        patch.modules = vec![
            ModuleDef::new("vco", "vco").with_position(100.0, 150.0),
            ModuleDef::new("lfo", "lfo").with_position(250.0, 50.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 150.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 150.0),
            ModuleDef::new("env", "adsr").with_position(400.0, 300.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 150.0),
        ];

        patch.cables = vec![
            CableDef::new("vco.saw", "vcf.in"),
            // LFO to filter cutoff - this is the key modulation
            CableDef::new("lfo.tri", "vcf.cutoff").with_attenuation(0.3),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("env.out", "vca.cv"),
        ];

        patch.parameters.insert("lfo.rate".into(), 0.3);
        patch.parameters.insert("vcf.cutoff".into(), 0.5);
        patch.parameters.insert("vcf.resonance".into(), 0.4);
        patch.parameters.insert("env.attack".into(), 0.01);
        patch.parameters.insert("env.decay".into(), 0.1);
        patch.parameters.insert("env.sustain".into(), 0.8);
        patch.parameters.insert("env.release".into(), 0.3);

        patch
    }

    /// FM synthesis basics
    ///
    /// One oscillator modulating another's frequency
    pub fn fm_basics() -> PatchDef {
        let mut patch = PatchDef::new("FM Basics")
            .with_author("Quiver")
            .with_description(
                "Tutorial: Basic FM (Frequency Modulation) synthesis. \
                 The modulator oscillator changes the frequency of the carrier, \
                 creating complex harmonic content.",
            )
            .with_tag("tutorial")
            .with_tag("intermediate")
            .with_tag("fm");

        patch.modules = vec![
            ModuleDef::new("modulator", "vco").with_position(100.0, 100.0),
            ModuleDef::new("carrier", "vco").with_position(100.0, 200.0),
            ModuleDef::new("fm_env", "adsr").with_position(100.0, 350.0),
            ModuleDef::new("vcf", "svf").with_position(250.0, 200.0),
            ModuleDef::new("vca", "vca").with_position(400.0, 200.0),
            ModuleDef::new("amp_env", "adsr").with_position(400.0, 350.0),
            ModuleDef::new("output", "stereo_output").with_position(550.0, 200.0),
        ];

        patch.cables = vec![
            // Modulator sine -> carrier FM input
            CableDef::new("modulator.sin", "carrier.fm"),
            // FM envelope controls modulation depth
            CableDef::new("fm_env.out", "modulator.fm").with_attenuation(0.3),
            // Carrier output through filter and VCA
            CableDef::new("carrier.sin", "vcf.in"),
            CableDef::new("vcf.lp", "vca.in"),
            CableDef::new("vca.out", "output.left"),
            CableDef::new("vca.out", "output.right"),
            CableDef::new("amp_env.out", "vca.cv"),
        ];

        patch.parameters.insert("vcf.cutoff".into(), 0.8);
        patch.parameters.insert("vcf.resonance".into(), 0.0);
        patch.parameters.insert("fm_env.attack".into(), 0.01);
        patch.parameters.insert("fm_env.decay".into(), 0.5);
        patch.parameters.insert("fm_env.sustain".into(), 0.2);
        patch.parameters.insert("fm_env.release".into(), 0.3);
        patch.parameters.insert("amp_env.attack".into(), 0.01);
        patch.parameters.insert("amp_env.decay".into(), 0.2);
        patch.parameters.insert("amp_env.sustain".into(), 0.6);
        patch.parameters.insert("amp_env.release".into(), 0.4);

        patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_library_list() {
        let presets = PresetLibrary::list();
        assert!(!presets.is_empty());
        assert!(presets.len() >= 12); // At least 12 presets defined
    }

    #[test]
    fn test_preset_library_by_category() {
        let bass_presets = PresetLibrary::by_category(PresetCategory::Bass);
        assert!(!bass_presets.is_empty());
        for preset in &bass_presets {
            assert_eq!(preset.category, PresetCategory::Bass);
        }

        let tutorials = PresetLibrary::by_category(PresetCategory::Tutorial);
        assert!(!tutorials.is_empty());
    }

    #[test]
    fn test_preset_library_by_tag() {
        let analog_presets = PresetLibrary::by_tag("analog");
        assert!(!analog_presets.is_empty());

        let beginner_presets = PresetLibrary::by_tag("beginner");
        assert!(!beginner_presets.is_empty());
    }

    #[test]
    fn test_preset_library_load() {
        // Test loading each preset
        let preset_names = [
            "Moog Bass",
            "303 Acid",
            "Juno Pad",
            "Sync Lead",
            "PWM Strings",
            "Metallic Ring",
            "Noise Sweep",
            "Wavefold Growl",
            "Basic Subtractive",
            "Envelope Basics",
            "Filter Modulation",
            "FM Basics",
        ];

        for name in preset_names {
            let patch = PresetLibrary::load(name);
            assert!(patch.is_some(), "Failed to load preset: {}", name);
            let patch = patch.unwrap();
            assert_eq!(patch.name, name);
            assert!(!patch.modules.is_empty());
            assert!(!patch.cables.is_empty());
        }
    }

    #[test]
    fn test_preset_load_nonexistent() {
        let patch = PresetLibrary::load("Nonexistent Preset");
        assert!(patch.is_none());
    }

    #[test]
    fn test_moog_bass_structure() {
        let patch = ClassicPresets::moog_bass();
        assert_eq!(patch.name, "Moog Bass");
        assert!(patch.modules.iter().any(|m| m.module_type == "vco"));
        assert!(patch.modules.iter().any(|m| m.module_type == "svf"));
        assert!(patch.modules.iter().any(|m| m.module_type == "vca"));
        assert!(patch.modules.iter().any(|m| m.module_type == "adsr"));
    }

    #[test]
    fn test_preset_serialization() {
        let patch = ClassicPresets::moog_bass();
        let json = patch.to_json().unwrap();
        assert!(json.contains("Moog Bass"));
        assert!(json.contains("vco"));

        // Round-trip
        let loaded = PatchDef::from_json(&json).unwrap();
        assert_eq!(loaded.name, patch.name);
        assert_eq!(loaded.modules.len(), patch.modules.len());
    }

    #[test]
    fn test_tutorial_presets_have_descriptions() {
        let tutorials = PresetLibrary::by_category(PresetCategory::Tutorial);
        for preset in tutorials {
            assert!(
                !preset.description.is_empty(),
                "Tutorial {} should have description",
                preset.name
            );
        }
    }

    #[test]
    fn test_preset_info_builder() {
        let info = PresetInfo::new("Test Preset", PresetCategory::Lead)
            .with_description("A test preset")
            .with_tag("test")
            .with_tag("example")
            .with_difficulty(3);

        assert_eq!(info.name, "Test Preset");
        assert_eq!(info.category, PresetCategory::Lead);
        assert_eq!(info.description, "A test preset");
        assert_eq!(info.tags.len(), 2);
        assert_eq!(info.difficulty, Some(3));
    }
}
