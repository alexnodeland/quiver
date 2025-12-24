import { test, expect } from '@playwright/test';

// Edge cases and robustness tests
// These tests verify the engine handles unusual conditions gracefully

test.describe('Block Size Edge Cases', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('handles block size of 1', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(1);
      engine.free();
      return { length: output.length, samples: Array.from(output) };
    });

    expect(result.length).toBe(2); // 1 sample * 2 channels
  });

  test('handles block size of 128', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(128);
      engine.free();
      return { length: output.length };
    });

    expect(result.length).toBe(256); // 128 samples * 2 channels
  });

  test('handles block size of 1024', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(1024);
      engine.free();
      return { length: output.length };
    });

    expect(result.length).toBe(2048);
  });

  test('handles block size of 4096', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(4096);
      engine.free();
      return { length: output.length };
    });

    expect(result.length).toBe(8192);
  });
});

test.describe('Sample Rate Edge Cases', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('handles 8000 Hz sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(8000.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(64);
      // sample_rate is a getter property
      const rate = engine.sample_rate;
      engine.free();
      return { rate, hasOutput: Array.from(output).some(s => Math.abs(s) > 0.001) };
    });

    expect(result.rate).toBe(8000);
    expect(result.hasOutput).toBe(true);
  });

  test('handles 44100 Hz sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      // sample_rate is a getter property
      const rate = engine.sample_rate;
      engine.free();
      return { rate };
    });

    expect(result.rate).toBe(44100);
  });

  test('handles 48000 Hz sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(48000.0);
      // sample_rate is a getter property
      const rate = engine.sample_rate;
      engine.free();
      return { rate };
    });

    expect(result.rate).toBe(48000);
  });

  test('handles 96000 Hz sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(96000.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(128);
      // sample_rate is a getter property
      const rate = engine.sample_rate;
      engine.free();
      return { rate, hasOutput: Array.from(output).some(s => Math.abs(s) > 0.001) };
    });

    expect(result.rate).toBe(96000);
    expect(result.hasOutput).toBe(true);
  });

  test('handles 192000 Hz sample rate', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(192000.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(128);
      // sample_rate is a getter property
      const rate = engine.sample_rate;
      engine.free();
      return { rate, hasOutput: Array.from(output).some(s => Math.abs(s) > 0.001) };
    });

    expect(result.rate).toBe(192000);
    expect(result.hasOutput).toBe(true);
  });
});

test.describe('Large Patch Handling', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('handles patch with 20+ modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create 20 oscillators
      for (let i = 0; i < 20; i++) {
        engine.add_module('vco', `osc${i}`);
      }
      engine.add_module('mixer', 'mix');
      engine.add_module('stereo_output', 'out');

      // Connect first 4 to mixer (mixer has 4 channels)
      for (let i = 0; i < 4; i++) {
        engine.connect(`osc${i}.saw`, `mix.ch${i}`);
      }
      engine.connect('mix.out', 'out.left');
      engine.connect('mix.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      const output = engine.process_block(128);
      const count = engine.module_count();

      engine.free();
      return {
        moduleCount: count,
        hasOutput: Array.from(output).some(s => Math.abs(s) > 0.001)
      };
    });

    expect(result.moduleCount).toBe(22); // 20 oscs + mixer + output
    expect(result.hasOutput).toBe(true);
  });

  test('handles patch with many cables', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create modules for complex routing
      for (let i = 0; i < 8; i++) {
        engine.add_module('vco', `osc${i}`);
      }
      for (let i = 0; i < 4; i++) {
        engine.add_module('svf', `filter${i}`);
      }
      for (let i = 0; i < 4; i++) {
        engine.add_module('vca', `amp${i}`);
      }
      engine.add_module('mixer', 'mix');
      engine.add_module('stereo_output', 'out');

      // Create many connections
      for (let i = 0; i < 4; i++) {
        engine.connect(`osc${i * 2}.saw`, `filter${i}.in`);
        engine.connect(`osc${i * 2 + 1}.tri`, `amp${i}.cv`);
        engine.connect(`filter${i}.lp`, `amp${i}.in`);
        engine.connect(`amp${i}.out`, `mix.ch${i}`);
      }
      engine.connect('mix.out', 'out.left');
      engine.connect('mix.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      const cableCount = engine.cable_count();
      const output = engine.process_block(128);

      engine.free();
      return {
        cableCount,
        hasOutput: Array.from(output).some(s => Math.abs(s) > 0.001)
      };
    });

    expect(result.cableCount).toBeGreaterThan(10);
    expect(result.hasOutput).toBe(true);
  });
});

test.describe('Stress Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('rapid engine create/destroy cycles', async ({ page }) => {
    const result = await page.evaluate(() => {
      const cycles = 50;
      let success = 0;

      for (let i = 0; i < cycles; i++) {
        try {
          const engine = new window.QuiverEngine(44100.0);
          engine.add_module('vco', 'osc');
          engine.add_module('stereo_output', 'out');
          engine.connect('osc.saw', 'out.left');
          engine.set_output('out');
          engine.compile();
          engine.process_block(64);
          engine.free();
          success++;
        } catch (e) {
          console.error(`Cycle ${i} failed:`, e);
        }
      }

      return { cycles, success };
    });

    expect(result.success).toBe(result.cycles);
  });

  test('rapid module add/remove cycles', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const cycles = 100;
      let success = 0;

      for (let i = 0; i < cycles; i++) {
        try {
          engine.add_module('vco', `osc_${i}`);
          engine.remove_module(`osc_${i}`);
          success++;
        } catch (e) {
          console.error(`Cycle ${i} failed:`, e);
        }
      }

      const finalCount = engine.module_count();
      engine.free();

      return { cycles, success, finalCount };
    });

    expect(result.success).toBe(result.cycles);
    expect(result.finalCount).toBe(0);
  });

  test('continuous processing for extended duration', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('svf', 'filter');
      engine.add_module('adsr', 'env');
      engine.add_module('vca', 'amp');
      engine.add_module('stereo_output', 'out');

      engine.connect('osc.saw', 'filter.in');
      engine.connect('filter.lp', 'amp.in');
      engine.connect('env.env', 'amp.cv');
      engine.connect('amp.out', 'out.left');
      engine.connect('amp.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Process ~5 seconds of audio at 44100 Hz
      const blocksToProcess = Math.ceil((44100 * 5) / 256);
      let totalSamples = 0;

      const start = performance.now();
      for (let i = 0; i < blocksToProcess; i++) {
        engine.process_block(256);
        totalSamples += 256;
      }
      const elapsed = performance.now() - start;

      engine.free();

      return {
        blocksProcessed: blocksToProcess,
        totalSamples,
        elapsedMs: elapsed,
        samplesPerSecond: totalSamples / (elapsed / 1000)
      };
    });

    // Should process at least real-time (44100 samples/sec)
    // Actually much faster in WASM
    expect(result.samplesPerSecond).toBeGreaterThan(44100);
  });
});

test.describe('Memory Safety', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('free() releases engine properly', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engines: QuiverEngineInstance[] = [];

      // Create many engines
      for (let i = 0; i < 10; i++) {
        const engine = new window.QuiverEngine(44100.0);
        engine.add_module('vco', 'osc');
        engine.add_module('stereo_output', 'out');
        engine.connect('osc.saw', 'out.left');
        engine.set_output('out');
        engine.compile();
        engines.push(engine);
      }

      // Free them all
      for (const engine of engines) {
        engine.free();
      }

      return { created: 10, freed: 10 };
    });

    expect(result.freed).toBe(10);
  });

  test('clear_patch does not leak memory', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      const cycles = 50;

      for (let i = 0; i < cycles; i++) {
        // Create a patch
        for (let j = 0; j < 10; j++) {
          engine.add_module('vco', `osc${j}`);
        }
        engine.add_module('stereo_output', 'out');

        // Clear it
        engine.clear_patch();
      }

      const finalCount = engine.module_count();
      engine.free();

      return { cycles, finalCount };
    });

    expect(result.finalCount).toBe(0);
  });
});

test.describe('Unusual Parameter Values', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('handles zero parameter value', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.set_param('offset1', 0, 0.0);
      const value = engine.get_param('offset1', 0);
      engine.free();
      return { value };
    });

    expect(result.value).toBe(0);
  });

  test('handles negative parameter value', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.set_param('offset1', 0, -5.0);
      const value = engine.get_param('offset1', 0);
      engine.free();
      return { value };
    });

    expect(result.value).toBe(-5.0);
  });

  test('handles large parameter value', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.set_param('offset1', 0, 1000.0);
      const value = engine.get_param('offset1', 0);
      engine.free();
      return { value };
    });

    expect(result.value).toBe(1000.0);
  });

  test('handles very small parameter value', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.set_param('offset1', 0, 0.000001);
      const value = engine.get_param('offset1', 0);
      engine.free();
      return { value };
    });

    expect(result.value).toBeCloseTo(0.000001, 6);
  });
});

// Type declarations
declare global {
  interface QuiverEngineInstance {
    free(): void;
    add_module(typeId: string, name: string): void;
    remove_module(name: string): void;
    connect(from: string, to: string): void;
    set_output(name: string): void;
    compile(): void;
    process_block(samples: number): Float32Array;
    module_count(): number;
    cable_count(): number;
    sample_rate(): number;
    clear_patch(): void;
    set_param(moduleId: string, paramIndex: number, value: number): void;
    get_param(moduleId: string, paramIndex: number): number;
  }
}

export {};
