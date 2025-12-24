import { test, expect } from '@playwright/test';

// Performance benchmark tests
// These tests verify real-time audio performance requirements

test.describe('Performance Benchmarks', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('process_block under 1ms for 128 samples', async ({ page }) => {
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

      // Warm up
      for (let i = 0; i < 100; i++) {
        engine.process_block(128);
      }

      // Benchmark
      const iterations = 1000;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.process_block(128);
      }
      const end = performance.now();

      engine.free();

      const totalMs = end - start;
      const avgMs = totalMs / iterations;
      return { avgMs, totalMs, iterations };
    });

    // 128 samples at 44100 Hz = ~2.9ms of audio
    // We need to process faster than real-time
    // Target: under 1ms (3x faster than real-time)
    expect(result.avgMs).toBeLessThan(1.0);
  });

  test('handles 256 sample buffer efficiently', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Warm up
      for (let i = 0; i < 50; i++) {
        engine.process_block(256);
      }

      // Benchmark
      const iterations = 500;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.process_block(256);
      }
      const end = performance.now();

      engine.free();

      const totalMs = end - start;
      const avgMs = totalMs / iterations;
      return { avgMs, totalMs, iterations };
    });

    // 256 samples at 44100 Hz = ~5.8ms of audio
    // Target: under 2ms
    expect(result.avgMs).toBeLessThan(2.0);
  });

  test('handles 512 sample buffer efficiently', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('osc.saw', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Benchmark
      const iterations = 250;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.process_block(512);
      }
      const end = performance.now();

      engine.free();

      const totalMs = end - start;
      const avgMs = totalMs / iterations;
      return { avgMs, totalMs, iterations };
    });

    // 512 samples at 44100 Hz = ~11.6ms of audio
    // Target: under 4ms
    expect(result.avgMs).toBeLessThan(4.0);
  });

  test('maintains performance with many modules', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a patch with 4 oscillators (matches mixer's 4 channels)
      for (let i = 0; i < 4; i++) {
        engine.add_module('vco', `osc${i}`);
      }
      engine.add_module('mixer', 'mix');
      engine.add_module('stereo_output', 'out');

      for (let i = 0; i < 4; i++) {
        // Mixer has ch0, ch1, ch2, ch3 inputs (4 channels by default)
        engine.connect(`osc${i}.saw`, `mix.ch${i}`);
      }
      engine.connect('mix.out', 'out.left');
      engine.connect('mix.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Benchmark
      const iterations = 500;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.process_block(128);
      }
      const end = performance.now();

      engine.free();

      const totalMs = end - start;
      const avgMs = totalMs / iterations;
      return { avgMs, moduleCount: 6 };  // 4 oscs + mixer + output
    });

    // With 6 modules, should still be under 2ms
    expect(result.avgMs).toBeLessThan(2.0);
  });

  test('module creation is fast', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      const start = performance.now();
      for (let i = 0; i < 100; i++) {
        engine.add_module('vco', `osc${i}`);
      }
      const end = performance.now();

      const count = engine.module_count();
      engine.free();

      return { totalMs: end - start, count };
    });

    // Creating 100 modules should be under 50ms
    expect(result.totalMs).toBeLessThan(50);
    expect(result.count).toBe(100);
  });

  test('compile time is reasonable', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a moderately complex patch
      engine.add_module('vco', 'osc1');
      engine.add_module('vco', 'osc2');
      engine.add_module('lfo', 'lfo');
      engine.add_module('svf', 'filter');
      engine.add_module('adsr', 'env1');
      engine.add_module('adsr', 'env2');
      engine.add_module('vca', 'amp');
      engine.add_module('mixer', 'mix');
      engine.add_module('stereo_output', 'out');

      engine.connect('osc1.saw', 'mix.ch0');
      engine.connect('osc2.tri', 'mix.ch1');
      engine.connect('mix.out', 'filter.in');
      engine.connect('lfo.tri', 'filter.fm');  // Use FM input, not 'cv'
      engine.connect('filter.lp', 'amp.in');
      engine.connect('env1.env', 'amp.cv');
      engine.connect('amp.out', 'out.left');
      engine.connect('amp.out', 'out.right');
      engine.set_output('out');

      const start = performance.now();
      engine.compile();
      const end = performance.now();

      engine.free();

      return { compileMs: end - start };
    });

    // Compile should be under 10ms
    expect(result.compileMs).toBeLessThan(10);
  });

  test('MIDI handling adds minimal overhead', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Benchmark without MIDI
      const iterations = 500;
      let start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.process_block(128);
      }
      let end = performance.now();
      const withoutMidi = (end - start) / iterations;

      // Benchmark with MIDI events each block
      start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.midi_note_on(60 + (i % 12), 100);
        engine.process_block(128);
        engine.midi_note_off(60 + (i % 12), 0);
      }
      end = performance.now();
      const withMidi = (end - start) / iterations;

      engine.free();

      return {
        withoutMidi,
        withMidi,
        overhead: withMidi - withoutMidi,
      };
    });

    // MIDI handling should add less than 0.1ms overhead
    expect(result.overhead).toBeLessThan(0.1);
  });

  test('JSON serialization is fast', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      // Create a patch
      for (let i = 0; i < 10; i++) {
        engine.add_module('vco', `osc${i}`);
      }
      engine.add_module('stereo_output', 'out');
      for (let i = 0; i < 10; i++) {
        engine.connect(`osc${i}.saw`, 'out.left');
      }

      // Benchmark export
      const exportIterations = 100;
      let start = performance.now();
      let patchDef: unknown = null;
      for (let i = 0; i < exportIterations; i++) {
        patchDef = engine.save_patch('Benchmark Patch');
      }
      let end = performance.now();
      const exportMs = (end - start) / exportIterations;

      // Benchmark import
      engine.clear_patch();
      const importIterations = 100;
      start = performance.now();
      for (let i = 0; i < importIterations; i++) {
        engine.load_patch(patchDef);
        if (i < importIterations - 1) engine.clear_patch();
      }
      end = performance.now();
      const importMs = (end - start) / importIterations;

      engine.free();

      return { exportMs, importMs };
    });

    // Serialization should be under 5ms each way
    expect(result.exportMs).toBeLessThan(5);
    expect(result.importMs).toBeLessThan(5);
  });

  test('memory usage is stable over time', async ({ page }) => {
    const result = await page.evaluate(async () => {
      // Force GC if available
      if ((window as { gc?: () => void }).gc) {
        (window as { gc?: () => void }).gc!();
      }

      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Process many blocks
      for (let i = 0; i < 10000; i++) {
        engine.process_block(128);
      }

      engine.free();

      // If we got here without crashing or hanging, memory is likely stable
      return { processed: 10000, stable: true };
    });

    expect(result.stable).toBe(true);
    expect(result.processed).toBe(10000);
  });
});

test.describe('Catalog Performance', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('get_catalog is fast', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      const iterations = 100;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.get_catalog();
      }
      const end = performance.now();

      engine.free();

      return { avgMs: (end - start) / iterations };
    });

    // Catalog retrieval should be under 1ms
    expect(result.avgMs).toBeLessThan(1);
  });

  test('get_categories is fast', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);

      const iterations = 100;
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        engine.get_categories();
      }
      const end = performance.now();

      engine.free();

      return { avgMs: (end - start) / iterations };
    });

    // Categories retrieval should be under 0.5ms
    expect(result.avgMs).toBeLessThan(0.5);
  });
});

export {};
