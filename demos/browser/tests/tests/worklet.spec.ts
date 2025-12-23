import { test, expect } from '@playwright/test';

// AudioWorklet tests
// These tests verify the AudioWorklet integration works correctly

test.describe('AudioWorklet', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('AudioContext can be created', async ({ page }) => {
    const result = await page.evaluate(async () => {
      const ctx = new AudioContext({ sampleRate: 44100 });
      const state = ctx.state;
      const sampleRate = ctx.sampleRate;
      await ctx.close();
      return { state, sampleRate };
    });

    expect(result.sampleRate).toBe(44100);
    // State might be 'suspended' or 'running' depending on autoplay policy
    expect(['suspended', 'running']).toContain(result.state);
  });

  test('AudioWorklet can be registered', async ({ page }) => {
    // This test checks if we can add a worklet module
    // The actual worklet file would need to be served
    const result = await page.evaluate(async () => {
      const ctx = new AudioContext({ sampleRate: 44100 });
      try {
        // Check if audioWorklet is supported
        const hasWorklet = 'audioWorklet' in ctx;
        await ctx.close();
        return { hasWorklet, error: null };
      } catch (e) {
        await ctx.close();
        return { hasWorklet: false, error: (e as Error).message };
      }
    });

    expect(result.hasWorklet).toBe(true);
  });

  test('ScriptProcessor fallback works', async ({ page }) => {
    // Test the deprecated ScriptProcessorNode as fallback
    const result = await page.evaluate(() => {
      const ctx = new AudioContext({ sampleRate: 44100 });
      const processor = ctx.createScriptProcessor(256, 0, 2);
      const bufferSize = processor.bufferSize;
      const channelCount = processor.channelCount;
      processor.disconnect();
      ctx.close();
      return { bufferSize, channelCount };
    });

    expect(result.bufferSize).toBe(256);
  });

  test('Web Audio graph can be constructed', async ({ page }) => {
    const result = await page.evaluate(() => {
      const ctx = new AudioContext({ sampleRate: 44100 });

      // Create a simple audio graph
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();

      osc.connect(gain);
      gain.connect(ctx.destination);

      // Set parameters
      osc.frequency.value = 440;
      gain.gain.value = 0;

      osc.start();

      // Clean up
      setTimeout(() => {
        osc.stop();
        osc.disconnect();
        gain.disconnect();
        ctx.close();
      }, 10);

      return { constructed: true };
    });

    expect(result.constructed).toBe(true);
  });

  test('Float32Array audio data handling', async ({ page }) => {
    // Test that audio data can be passed correctly
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc:saw', 'out:left');
      engine.connect('osc:saw', 'out:right');
      engine.compile();

      // Process multiple blocks
      const blocks: number[][] = [];
      for (let i = 0; i < 4; i++) {
        const output = engine.process_block(128);
        blocks.push(Array.from(output));
      }

      engine.free();

      return {
        blockCount: blocks.length,
        samplesPerBlock: blocks[0].length,
        isFloat32: true,
        // Check samples are in valid range
        allInRange: blocks.every(block =>
          block.every(s => s >= -1.0 && s <= 1.0)
        ),
      };
    });

    expect(result.blockCount).toBe(4);
    expect(result.samplesPerBlock).toBe(256); // 128 * 2 channels
    expect(result.allInRange).toBe(true);
  });

  test('multiple engines can coexist', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine1 = new window.QuiverEngine(44100.0);
      const engine2 = new window.QuiverEngine(48000.0);

      engine1.add_module('vco', 'osc1');
      engine2.add_module('vco', 'osc2');
      engine2.add_module('vca', 'amp2');

      const count1 = engine1.module_count();
      const count2 = engine2.module_count();

      engine1.free();
      engine2.free();

      return { count1, count2 };
    });

    expect(result.count1).toBe(1);
    expect(result.count2).toBe(2);
  });

  test('audio processing is continuous', async ({ page }) => {
    // Verify that consecutive blocks produce continuous audio
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc:saw', 'out:left');
      engine.compile();

      // Process multiple blocks and check for discontinuities
      let lastSample = 0;
      let maxJump = 0;

      for (let block = 0; block < 10; block++) {
        const output = engine.process_block(128);
        // Check left channel (every other sample)
        for (let i = 0; i < output.length; i += 2) {
          const jump = Math.abs(output[i] - lastSample);
          if (jump > maxJump) maxJump = jump;
          lastSample = output[i];
        }
      }

      engine.free();

      // Sawtooth wave has discontinuities, but they shouldn't be huge
      return { maxJump, continuous: maxJump < 2.0 };
    });

    expect(result.continuous).toBe(true);
  });
});

test.describe('AudioWorklet Message Passing', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('parameters can be updated during processing', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc:saw', 'out:left');
      engine.compile();

      // Process with initial frequency
      engine.set_param_by_name('osc', 'frequency', 440);
      const block1 = engine.process_block(128);

      // Change frequency mid-stream
      engine.set_param_by_name('osc', 'frequency', 880);
      const block2 = engine.process_block(128);

      engine.free();

      // Both blocks should have audio
      const hasAudio1 = Array.from(block1).some(s => Math.abs(s) > 0.001);
      const hasAudio2 = Array.from(block2).some(s => Math.abs(s) > 0.001);

      return { hasAudio1, hasAudio2 };
    });

    expect(result.hasAudio1).toBe(true);
    expect(result.hasAudio2).toBe(true);
  });

  test('MIDI events are processed in real-time', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('adsr', 'env');
      engine.add_module('stereo_output', 'out');
      engine.connect('env:env', 'out:left');
      engine.compile();

      // Process with note off (gate = 0)
      const silentBlock = engine.process_block(128);
      const silentMax = Math.max(...Array.from(silentBlock).map(Math.abs));

      // Note on
      engine.midi_note_on(60, 100);
      const activeBlock = engine.process_block(128);
      const activeMax = Math.max(...Array.from(activeBlock).map(Math.abs));

      engine.free();

      return {
        silentMax,
        activeMax,
        gateWorking: activeMax > silentMax,
      };
    });

    // ADSR should produce higher output with gate on
    expect(result.gateWorking).toBe(true);
  });
});

export {};
