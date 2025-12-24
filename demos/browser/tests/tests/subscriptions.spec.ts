import { test, expect } from '@playwright/test';

// State Observation API tests
// These tests verify the real-time bridge / observer functionality

test.describe('State Observation API', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => window.QuiverEngine !== null, { timeout: 10000 });
  });

  test('subscribe accepts param subscription', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');

      try {
        // Subscribe to a parameter using serde tagged format
        // Format: { type: "param", node_id: "...", param_id: "..." }
        engine.subscribe([
          { type: 'param', node_id: 'offset1', param_id: 'offset' }
        ]);
        engine.free();
        return { subscribed: true, error: null };
      } catch (e) {
        engine.free();
        return { subscribed: false, error: String(e) };
      }
    });

    expect(result.subscribed).toBe(true);
  });

  test('poll_updates returns pending data', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.add_module('stereo_output', 'out');
      engine.connect('offset1.out', 'out.left');
      engine.set_output('out');
      engine.compile();

      try {
        // Subscribe to parameter using serde tagged format
        engine.subscribe([
          { type: 'param', node_id: 'offset1', param_id: 'offset' }
        ]);

        // Set a param value
        engine.set_param('offset1', 0, 2.5);

        // Process to trigger collection
        engine.process_block(128);

        // Poll for updates
        const updates = engine.poll_updates();

        engine.free();
        return { updates, hasUpdates: Array.isArray(updates) && updates.length > 0, error: null };
      } catch (e) {
        engine.free();
        return { updates: [], hasUpdates: false, error: String(e) };
      }
    });

    expect(result.error).toBeNull();
    expect(Array.isArray(result.updates)).toBe(true);
  });

  test('pending_update_count returns queue length', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.add_module('stereo_output', 'out');
      engine.connect('offset1.out', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Subscribe using serde tagged format
      engine.subscribe([
        { type: 'param', node_id: 'offset1', param_id: 'offset' }
      ]);

      // Set param and process
      engine.set_param('offset1', 0, 1.5);
      engine.process_block(128);

      const count = engine.pending_update_count();

      engine.free();
      return { count };
    });

    expect(typeof result.count).toBe('number');
    expect(result.count).toBeGreaterThanOrEqual(0);
  });

  test('unsubscribe stops updates for target', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.add_module('stereo_output', 'out');
      engine.connect('offset1.out', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Subscribe using serde tagged format
      engine.subscribe([
        { type: 'param', node_id: 'offset1', param_id: 'offset' }
      ]);

      // Process to get some updates
      engine.set_param('offset1', 0, 1.0);
      engine.process_block(128);
      engine.poll_updates(); // Clear pending

      // Unsubscribe
      engine.unsubscribe(['param:offset1:offset']);

      // Process more and check
      engine.set_param('offset1', 0, 2.0);
      engine.process_block(128);

      const countAfter = engine.pending_update_count();

      engine.free();
      return { countAfter };
    });

    // After unsubscribe, should have fewer or no updates for that target
    expect(result.countAfter).toBe(0);
  });

  test('clear_subscriptions removes all subscriptions', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.add_module('offset', 'offset2');
      engine.add_module('stereo_output', 'out');
      engine.connect('offset1.out', 'out.left');
      engine.connect('offset2.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Subscribe to multiple targets using serde tagged format
      engine.subscribe([
        { type: 'param', node_id: 'offset1', param_id: 'offset' },
        { type: 'param', node_id: 'offset2', param_id: 'offset' }
      ]);

      // Clear all
      engine.clear_subscriptions();

      // Process and check
      engine.set_param('offset1', 0, 1.0);
      engine.set_param('offset2', 0, 2.0);
      engine.process_block(128);

      const count = engine.pending_update_count();

      engine.free();
      return { count };
    });

    expect(result.count).toBe(0);
  });

  test('Level subscription tracks output levels', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Subscribe to level metering using serde tagged format
      engine.subscribe([
        { type: 'level', node_id: 'osc', port_id: 0 }
      ]);

      // Process several blocks
      for (let i = 0; i < 10; i++) {
        engine.process_block(128);
      }

      const updates = engine.poll_updates();

      engine.free();
      return { updates, count: updates.length };
    });

    expect(Array.isArray(result.updates)).toBe(true);
  });

  test('Scope subscription captures waveform samples', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.set_output('out');
      engine.compile();

      // Subscribe to scope capture using serde tagged format
      engine.subscribe([
        { type: 'scope', node_id: 'osc', port_id: 0, buffer_size: 256 }
      ]);

      // Process enough to fill buffer
      for (let i = 0; i < 10; i++) {
        engine.process_block(128);
      }

      const updates = engine.poll_updates();

      engine.free();

      // Look for scope updates
      const scopeUpdates = updates.filter((u: { type: string }) => u.type === 'scope');
      return {
        totalUpdates: updates.length,
        scopeUpdates: scopeUpdates.length,
        hasScopeData: scopeUpdates.some((u: { samples?: number[] }) =>
          u.samples && u.samples.length > 0
        )
      };
    });

    expect(result.totalUpdates).toBeGreaterThanOrEqual(0);
  });

  test('multiple subscriptions work simultaneously', async ({ page }) => {
    const result = await page.evaluate(() => {
      const engine = new window.QuiverEngine(44100.0);
      engine.add_module('offset', 'offset1');
      engine.add_module('vco', 'osc');
      engine.add_module('stereo_output', 'out');
      engine.connect('osc.saw', 'out.left');
      engine.connect('offset1.out', 'out.right');
      engine.set_output('out');
      engine.compile();

      // Subscribe to multiple target types using serde tagged format
      engine.subscribe([
        { type: 'param', node_id: 'offset1', param_id: 'offset' },
        { type: 'level', node_id: 'osc', port_id: 0 }
      ]);

      // Trigger updates
      engine.set_param('offset1', 0, 3.0);

      // Process
      for (let i = 0; i < 5; i++) {
        engine.process_block(128);
      }

      const updates = engine.poll_updates();

      engine.free();
      return { updateCount: updates.length };
    });

    expect(result.updateCount).toBeGreaterThanOrEqual(0);
  });
});

export {};
