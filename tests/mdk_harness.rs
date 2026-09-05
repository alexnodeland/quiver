//! Dogfood the Module Development Kit (Q131).
//!
//! `src/mdk.rs` ships `ModuleTestHarness::run_all` (port-spec, reset-determinism,
//! sample-rate, zero-input, stability, NaN/Inf output, NaN/Inf input recovery,
//! output-range checks) but nothing in the library ran it against its own
//! modules. This test instantiates EVERY module registered in `ModuleRegistry`
//! via the registry and runs the full harness over it, so the kit's contract is
//! verified against the modules it ships with. The `stability` and
//! `output_range` checks drive audio-kind inputs with a live tone (not silence)
//! and `nan_recovery` injects NaN/±Inf on audio inputs then a clean signal, so
//! every module gets real signal-path and feedback-sanitization coverage.
//!
//! A small, documented allowlist covers modules that legitimately violate a
//! harness assumption rather than having a bug. The test also asserts every
//! allowlisted `(type_id, check)` STILL fails, so the allowlist cannot rot into
//! silently masking a regression.
#![cfg(feature = "std")]

use quiver::prelude::*;

/// `(type_id, failing_check_name)` — modules that fail one harness check for a
/// documented, known reason rather than a fresh regression. The test asserts
/// every entry STILL fails (see the stale-allowlist guard below), so an entry
/// cannot silently mask a module that has since been fixed or changed.
///
/// Currently empty. `noise` / `reset_clears_state` used to be listed because the
/// white-noise output came from the process-global RNG; the harness now seeds
/// every module (`GraphModule::seed`, Q-N2) and stochastic modules own their
/// stream, so `reset()` rewinds it and the check passes. All modules with
/// feedback/detector state pass `nan_recovery`: every audio-kind input feeding
/// such state goes through `sanitize_audio`, so a non-finite sample cannot
/// latch. Any new entry here needs a justification of the same weight.
const ALLOWLIST: &[(&str, &str)] = &[];

/// Adapter so a `Box<dyn GraphModule>` from the registry satisfies the
/// `M: GraphModule` bound on `ModuleTestHarness` (there is no blanket impl for
/// `Box<dyn GraphModule>`).
struct Boxed(Box<dyn GraphModule>);

impl GraphModule for Boxed {
    fn port_spec(&self) -> &PortSpec {
        self.0.port_spec()
    }
    fn tick(&mut self, i: &PortValues, o: &mut PortValues) {
        self.0.tick(i, o)
    }
    fn reset(&mut self) {
        self.0.reset()
    }
    fn set_sample_rate(&mut self, sr: f64) {
        self.0.set_sample_rate(sr)
    }
    fn seed(&mut self, seed: u64) {
        self.0.seed(seed)
    }
    fn type_id(&self) -> &'static str {
        self.0.type_id()
    }
}

#[test]
fn every_registered_module_passes_the_harness() {
    let registry = ModuleRegistry::new();
    let mut ids: Vec<String> = registry.list_modules().map(|m| m.type_id.clone()).collect();
    ids.sort();
    assert!(
        ids.len() >= 60,
        "registry unexpectedly small: {}",
        ids.len()
    );

    let mut unexpected = Vec::new();
    // Track which allowlist entries actually fired, so we can detect stale ones.
    let mut allowlist_hits = vec![false; ALLOWLIST.len()];

    for id in &ids {
        let module = registry
            .instantiate(id, 44_100.0)
            .expect("registered module");
        let mut harness = ModuleTestHarness::new(Boxed(module), 44_100.0);
        let suite = harness.run_all();
        assert_eq!(suite.results.len(), 8, "{id}: harness should run 8 checks");

        for r in &suite.results {
            if r.passed {
                continue;
            }
            if let Some(pos) = ALLOWLIST.iter().position(|&(t, c)| t == id && c == r.name) {
                allowlist_hits[pos] = true;
            } else {
                unexpected.push(format!("{} :: {} :: {:?}", id, r.name, r.error));
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "unexpected harness failures ({}):\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );

    // The allowlist must not rot: every entry must still correspond to a real
    // failure (either the module was removed, or the harness assumption changed).
    for (i, &(t, c)) in ALLOWLIST.iter().enumerate() {
        assert!(
            allowlist_hits[i],
            "stale allowlist entry ({t}, {c}): it no longer fails — remove it"
        );
    }
}

#[test]
fn audio_analysis_measures_a_known_signal() {
    // Dogfood AudioAnalysis (also shipped by the MDK) against a synthesized tone.
    let sr = 44_100.0;
    let freq = 440.0;
    let samples: Vec<f64> = (0..sr as usize)
        .map(|n| (core::f64::consts::TAU * freq * n as f64 / sr).sin())
        .collect();

    let rms = AudioAnalysis::rms(&samples);
    assert!(
        (rms - core::f64::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "sine RMS ~0.707, got {rms}"
    );
    assert!((AudioAnalysis::peak(&samples) - 1.0).abs() < 0.01);
    assert!(AudioAnalysis::dc_offset(&samples).abs() < 0.01);
    let est = AudioAnalysis::estimate_frequency(&samples, sr).expect("frequency");
    assert!(
        (est - freq).abs() < 5.0,
        "estimated {est} Hz, expected {freq} Hz"
    );
}
