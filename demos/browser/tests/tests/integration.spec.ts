import { test, expect } from '@playwright/test';

// Integration tests for npm package consumption
// These tests verify the full WASM + TypeScript integration works correctly

test.describe('Package Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('WASM module exports expected API', async ({ page }) => {
    const exports = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const api = {
        // Core methods
        hasAddModule: typeof engine.add_module === 'function',
        hasRemoveModule: typeof engine.remove_module === 'function',
        hasConnect: typeof engine.connect === 'function',
        hasDisconnect: typeof engine.disconnect === 'function',
        hasSetParam: typeof engine.set_param === 'function',
        hasSetOutput: typeof engine.set_output === 'function',
        hasCompile: typeof engine.compile === 'function',
        hasReset: typeof engine.reset === 'function',
        hasTick: typeof engine.tick === 'function',
        hasProcessBlock: typeof engine.process_block === 'function',

        // Catalog methods
        hasGetCatalog: typeof engine.get_catalog === 'function',
        hasGetCategories: typeof engine.get_categories === 'function',
        hasGetPortSpec: typeof engine.get_port_spec === 'function',

        // MIDI methods
        hasMidiNoteOn: typeof engine.midi_note_on === 'function',
        hasMidiNoteOff: typeof engine.midi_note_off === 'function',
        hasMidiCc: typeof engine.midi_cc === 'function',
        hasMidiPitchBend: typeof engine.midi_pitch_bend === 'function',
        hasCreateMidiInput: typeof engine.create_midi_input === 'function',

        // Observer methods
        hasSubscribe: typeof engine.subscribe === 'function',
        hasUnsubscribe: typeof engine.unsubscribe === 'function',
        hasPollUpdates: typeof engine.poll_updates === 'function',

        // State methods
        hasModuleCount: typeof engine.module_count === 'function',
        hasCableCount: typeof engine.cable_count === 'function',
        hasSampleRate: typeof engine.sample_rate !== 'undefined',
      };
      engine.free();
      return api;
    });

    // Verify all expected methods exist
    expect(exports.hasAddModule).toBe(true);
    expect(exports.hasRemoveModule).toBe(true);
    expect(exports.hasConnect).toBe(true);
    expect(exports.hasDisconnect).toBe(true);
    expect(exports.hasSetParam).toBe(true);
    expect(exports.hasSetOutput).toBe(true);
    expect(exports.hasCompile).toBe(true);
    expect(exports.hasReset).toBe(true);
    expect(exports.hasTick).toBe(true);
    expect(exports.hasProcessBlock).toBe(true);
    expect(exports.hasGetCatalog).toBe(true);
    expect(exports.hasGetCategories).toBe(true);
    expect(exports.hasGetPortSpec).toBe(true);
    expect(exports.hasMidiNoteOn).toBe(true);
    expect(exports.hasMidiNoteOff).toBe(true);
    expect(exports.hasMidiCc).toBe(true);
    expect(exports.hasMidiPitchBend).toBe(true);
    expect(exports.hasCreateMidiInput).toBe(true);
    expect(exports.hasSubscribe).toBe(true);
    expect(exports.hasUnsubscribe).toBe(true);
    expect(exports.hasPollUpdates).toBe(true);
    expect(exports.hasModuleCount).toBe(true);
    expect(exports.hasCableCount).toBe(true);
    expect(exports.hasSampleRate).toBe(true);
  });

  test('MIDI input routing creates modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const beforeCount = engine.module_count();

      // Create MIDI inputs
      engine.create_midi_input();

      const afterCount = engine.module_count();
      engine.free();
      return { before: beforeCount, after: afterCount };
    });

    // Should have created 5 MIDI input modules (voct, gate, velocity, pitch_bend, mod_wheel)
    expect(result.after - result.before).toBe(5);
  });

  test('MIDI CC input creates module', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const beforeCount = engine.module_count();

      // Create CC input for mod wheel (CC 1)
      engine.create_midi_cc_input(1);

      const afterCount = engine.module_count();
      engine.free();
      return { before: beforeCount, after: afterCount };
    });

    expect(result.after - result.before).toBe(1);
  });

  test('full synth patch can be created and processed', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a simple subtractive synth voice
      engine.add_module('vco', 'osc');
      engine.add_module('svf', 'filter');
      engine.add_module('adsr', 'env');
      engine.add_module('vca', 'amp');
      engine.add_module('stereo_output', 'out');

      // Connect modules
      engine.connect('osc.saw', 'filter.in');
      engine.connect('filter.lp', 'amp.in');
      engine.connect('env.env', 'amp.cv');
      engine.connect('amp.out', 'out.left');
      engine.connect('amp.out', 'out.right');

      // Set output and compile
      engine.set_output('out');
      engine.compile();

      // Process a block of audio
      const samples = engine.process_block(128);

      engine.free();
      return {
        moduleCount: 5,
        samplesLength: samples.length,
        isFloat32Array: samples instanceof Float32Array,
      };
    });

    expect(result.moduleCount).toBe(5);
    expect(result.samplesLength).toBe(256); // 128 samples * 2 channels
    expect(result.isFloat32Array).toBe(true);
  });

  test('port specifications are correctly structured', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const portSpec = engine.get_port_spec('vco');
      engine.free();
      return portSpec;
    });

    // VCO should have specific ports
    expect(result.inputs).toBeDefined();
    expect(result.outputs).toBeDefined();
    expect(Array.isArray(result.inputs)).toBe(true);
    expect(Array.isArray(result.outputs)).toBe(true);

    // Check for expected VCO ports
    const inputNames = result.inputs.map((p: { name: string }) => p.name);
    const outputNames = result.outputs.map((p: { name: string }) => p.name);

    expect(inputNames).toContain('voct');
    expect(outputNames).toContain('saw');
    expect(outputNames).toContain('sin');   // Not 'sine'
    expect(outputNames).toContain('sqr');   // Not 'square'
  });

  test('observer subscriptions work correctly', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a simple patch with offset module (which has observable parameters)
      engine.add_module('offset', 'knob');
      engine.add_module('stereo_output', 'out');
      engine.connect('knob.out', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Subscribe to parameter changes (new API takes array of subscription targets)
      // Use param_id as string matching the parameter name
      engine.subscribe([{ type: 'param', node_id: 'knob', param_id: 'offset' }]);

      // Set a parameter
      engine.set_param('knob', 0, 2.5);

      // Process some audio to trigger updates
      engine.process_block(128);

      // Poll updates (renamed from drain_updates)
      const updates = engine.poll_updates();

      engine.free();
      return {
        hasUpdates: updates.length > 0,
        updateCount: updates.length,
      };
    });

    // Should have at least one update from the parameter change
    expect(result.hasUpdates).toBe(true);
  });

  test('patch serialization roundtrip works', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a patch
      engine.add_module('vco', 'osc');
      engine.add_module('vca', 'amp');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'amp.in');
      engine.connect('amp.out', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Serialize
      const json = engine.save_patch('Test Patch');

      // Create new engine and load
      const engine2 = new window.QuiverEngine(44100.0);
      engine2.load_patch(json);
      engine2.compile();

      const moduleCount = engine2.module_count();
      const cableCount = engine2.cable_count();

      engine.free();
      engine2.free();

      return { moduleCount, cableCount };
    });

    expect(result.moduleCount).toBe(3);
    expect(result.cableCount).toBe(2);
  });

  test('safety clamp prevents dangerous output levels', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create an offset module that could produce high values
      engine.add_module('offset', 'loud');
      engine.add_module('stereo_output', 'out');
      engine.connect('loud.out', 'out.left');
      engine.connect('loud.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Set an extreme value (would be >10V in modular terms)
      engine.set_param('loud', 0, 100.0);

      // Process audio
      const samples = engine.process_block(128);

      // Check all samples are within safe range
      let maxAbs = 0;
      for (let i = 0; i < samples.length; i++) {
        maxAbs = Math.max(maxAbs, Math.abs(samples[i]));
      }

      engine.free();
      return { maxAbs, clamped: maxAbs <= 10.0 };
    });

    expect(result.clamped).toBe(true);
    expect(result.maxAbs).toBeLessThanOrEqual(10.0);
  });
});
