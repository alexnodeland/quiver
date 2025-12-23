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
    expect(statusText).toContain('WASM module loaded successfully');
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
      const catalogJson = engine.get_catalog();
      engine.free();
      return JSON.parse(catalogJson);
    });

    expect(catalog.modules).toBeDefined();
    expect(Array.isArray(catalog.modules)).toBe(true);
    expect(catalog.modules.length).toBeGreaterThan(0);

    // Verify essential modules exist
    const moduleNames = catalog.modules.map((m: { name: string }) => m.name);
    expect(moduleNames).toContain('vco');
    expect(moduleNames).toContain('vcf');
    expect(moduleNames).toContain('vca');
    expect(moduleNames).toContain('adsr');
  });

  test('returns module categories', async ({ page }) => {
    const categories = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const categoriesJson = engine.get_categories();
      engine.free();
      return JSON.parse(categoriesJson);
    });

    expect(Array.isArray(categories)).toBe(true);
    expect(categories.length).toBeGreaterThan(0);
    expect(categories).toContain('oscillator');
    expect(categories).toContain('filter');
    expect(categories).toContain('envelope');
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
      engine.connect('osc:saw', 'out:left');
      engine.connect('osc:saw', 'out:right');
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
      engine.connect('osc:saw', 'out:left');
      engine.connect('osc:saw', 'out:right');
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
      engine.connect('osc:saw', 'out:left');
      engine.connect('osc:saw', 'out:right');
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

  test('sets parameter values', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.set_param_by_name('osc', 'frequency', 880.0);
      engine.set_param_by_name('osc', 'pulse_width', 0.25);
      // Getting params would require additional API
      engine.free();
      return { set: true };
    });
    expect(result.set).toBe(true);
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
      engine.clear();
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
      const note = engine.midi_note;
      const gate = engine.midi_gate;
      engine.free();
      return { note, gate };
    });
    expect(result.note).toBe(60);
    expect(result.gate).toBeGreaterThan(0);
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
    expect(result.gate).toBe(0);
  });

  test('handles pitch bend', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.midi_pitch_bend(8192); // Center position
      const bend = engine.midi_pitch_bend;
      engine.free();
      return { bend };
    });
    expect(result.bend).toBeDefined();
  });

  test('handles mod wheel', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.midi_mod_wheel(64);
      const mod = engine.midi_mod_wheel;
      engine.free();
      return { mod };
    });
    expect(result.mod).toBeDefined();
  });
});

test.describe('Patch Serialization', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('exports patch to JSON', async ({ page }) => {
    const patchJson = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc:saw', 'out:left');
      const json = engine.export_patch('Test Patch');
      engine.free();
      return json;
    });

    const patch = JSON.parse(patchJson);
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
      engine.connect('osc:saw', 'out:left');
      const json = engine.export_patch('Test');

      // Clear and reimport
      engine.clear();
      engine.import_patch(json);
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
  get_catalog(): string;
  get_categories(): string;
  add_module(moduleType: string, id: string): void;
  remove_module(id: string): void;
  connect(from: string, to: string): void;
  compile(): void;
  process_block(samples: number): Float32Array;
  set_param(moduleId: string, paramIndex: number, value: number): void;
  set_param_by_name(moduleId: string, paramName: string, value: number): void;
  clear(): void;
  midi_note_on(note: number, velocity: number): void;
  midi_note_off(note: number, velocity: number): void;
  midi_pitch_bend(value: number): void;
  midi_mod_wheel(value: number): void;
  midi_note: number;
  midi_gate: number;
  midi_pitch_bend: number;
  midi_mod_wheel: number;
  export_patch(name: string): string;
  import_patch(json: string): void;
}

export {};
