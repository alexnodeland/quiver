import { test, expect } from '@playwright/test';

// QuiverEngine WASM tests
// These tests verify the core WASM bindings work correctly in browser

test.describe('QuiverEngine', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    // Wait for WASM to initialize
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('WASM module loads successfully', async ({ page }) => {
    const statusText = await page.locator('#status').textContent();
    // Status shows "All tests passed!" after inline tests complete
    expect(statusText).toMatch(/WASM module loaded successfully|All tests passed/);
  });

  test('creates engine at specified sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(48000.0);
      const count = engine.module_count();
      engine.free();
      return { created: true, moduleCount: count };
    });
    expect(result.created).toBe(true);
    expect(result.moduleCount).toBe(0);
  });

  test('returns module catalog', async ({ page }) => {
    const catalog = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const catalog = engine.get_catalog();
      engine.free();
      return catalog;
    });

    expect(catalog.modules).toBeDefined();
    expect(Array.isArray(catalog.modules)).toBe(true);
    expect(catalog.modules.length).toBeGreaterThan(0);

    // Verify essential modules exist (use type_id, not display name)
    const moduleTypeIds = catalog.modules.map((m: { type_id: string }) => m.type_id);
    expect(moduleTypeIds).toContain('vco');
    expect(moduleTypeIds).toContain('svf');  // 'vcf' is actually 'svf' type
    expect(moduleTypeIds).toContain('vca');
    expect(moduleTypeIds).toContain('adsr');
  });

  test('returns module categories', async ({ page }) => {
    const categories = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const categories = engine.get_categories();
      engine.free();
      return categories;
    });

    expect(Array.isArray(categories)).toBe(true);
    expect(categories.length).toBeGreaterThan(0);
    expect(categories).toContain('Oscillators');
    expect(categories).toContain('Filters');
    expect(categories).toContain('Envelopes');
  });

  test('adds modules to graph', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc1');
      engine.add_module('vca', 'amp1');
      const count = engine.module_count();
      engine.free();
      return count;
    });
    expect(result).toBe(2);
  });

  test('connects modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      const cableCount = engine.cable_count();
      engine.free();
      return cableCount;
    });
    expect(result).toBe(2);
  });

  test('compiles graph successfully', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      engine.compile();
      return { compiled: true };
    });
    expect(result.compiled).toBe(true);
  });

  test('processes audio block', async ({ page }) => {
    const samples = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      engine.set_output('out');
      engine.compile();
      const output = engine.process_block(128);
      const result = Array.from(output);
      engine.free();
      return result;
    });

    expect(samples.length).toBe(256); // 128 samples * 2 channels
    // Check that we got actual audio (not silence)
    const hasAudio = samples.some(s => Math.abs(s) > 0.001);
    expect(hasAudio).toBe(true);
  });

  test('sets parameter values via set_param', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      // Use set_param with param ID directly (more reliable)
      engine.set_param('offset1', 0, 1.5);
      const value = engine.get_param('offset1', 0);
      engine.free();
      return { set: true, value };
    });
    expect(result.set).toBe(true);
    expect(result.value).toBe(1.5);
  });

  test('removes modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc1');
      engine.add_module('vco', 'osc2');
      const countBefore = engine.module_count();
      engine.remove_module('osc1');
      const countAfter = engine.module_count();
      engine.free();
      return { before: countBefore, after: countAfter };
    });
    expect(result.before).toBe(2);
    expect(result.after).toBe(1);
  });

  test('clears all modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc1');
      engine.add_module('vco', 'osc2');
      engine.add_module('vca', 'amp');
      engine.clear_patch();
      const count = engine.module_count();
      engine.free();
      return count;
    });
    expect(result).toBe(0);
  });
});

test.describe('MIDI', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('handles note on', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.midi_note_on(60, 100);
      // midi_note returns V/Oct where 0V = C4 (MIDI 60)
      // So MIDI 60 -> 0V, MIDI 72 -> 1V, etc.
      const note = engine.midi_note;
      const gate = engine.midi_gate;
      engine.free();
      return { note, gate };
    });
    // MIDI note 60 (C4) = 0 V/Oct
    expect(result.note).toBe(0);
    expect(result.gate).toBe(true);
  });

  test('handles note off', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.midi_note_on(60, 100);
      engine.midi_note_off(60, 0);
      const gate = engine.midi_gate;
      engine.free();
      return { gate };
    });
    expect(result.gate).toBe(false);
  });

  test('handles pitch bend', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.midi_pitch_bend(0.5); // Takes -1 to 1, not raw MIDI value
      const bend = engine.pitch_bend; // Getter is 'pitch_bend', not 'midi_pitch_bend'
      engine.free();
      return { bend };
    });
    expect(result.bend).toBe(0.5);
  });

  test('handles mod wheel', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      // Mod wheel is CC #1
      engine.midi_cc(1, 64);
      const mod = engine.get_midi_cc(1);
      engine.free();
      return { mod };
    });
    // CC value 64 normalized to 0-1 = 64/127 ≈ 0.504
    expect(result.mod).toBeCloseTo(64 / 127, 2);
  });
});

test.describe('Patch Serialization', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('exports patch to JSON', async ({ page }) => {
    const patch = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      // save_patch returns a JS object, not a string
      const patchDef = engine.save_patch('Test Patch');
      engine.free();
      return patchDef;
    });

    expect(patch.name).toBe('Test Patch');
    expect(patch.modules).toBeDefined();
    expect(patch.cables).toBeDefined();
  });

  test('imports patch from JSON', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create and export a patch
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      const patchDef = engine.save_patch('Test');

      // Clear and reimport
      engine.clear_patch();
      engine.load_patch(patchDef);
      const moduleCount = engine.module_count();
      engine.free();
      return { moduleCount };
    });

    expect(result.moduleCount).toBe(2);
  });
});

// TypeScript declarations for the browser globals
declare global {
  interface Window {
    QuiverEngine: {
      new (sampleRate: number): QuiverEngineInstance;
    } | null;
    engine: QuiverEngineInstance | null;
    testResults: Record<string, { pass: boolean; message: string }>;
  }
}

interface QuiverEngineInstance {
  free(): void;
  module_count(): number;
  cable_count(): number;
  get_catalog(): { modules: { name: string; category: string }[] };
  get_categories(): string[];
  add_module(moduleType: string, id: string): void;
  remove_module(id: string): void;
  connect(from: string, to: string): void;
  set_output(name: string): void;
  compile(): void;
  process_block(samples: number): Float32Array;
  set_param(moduleId: string, paramIndex: number, value: number): void;
  set_param_by_name(moduleId: string, paramName: string, value: number): void;
  clear_patch(): void;
  midi_note_on(note: number, velocity: number): void;
  midi_note_off(note: number, velocity: number): void;
  midi_pitch_bend(value: number): void;
  midi_cc(cc: number, value: number): void;
  get_midi_cc(cc: number): number;
  midi_note: number;
  midi_gate: boolean;
  midi_velocity: number;
  pitch_bend: number;
  save_patch(name: string): { name: string; modules: unknown[]; cables: unknown[] };
  load_patch(patchDef: unknown): void;
}

export {};
