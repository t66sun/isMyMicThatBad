import assert from 'node:assert/strict';
import { chromium } from '../src/app/node_modules/playwright-core/index.mjs';

const seconds = Number(process.argv[2] ?? 1800);
assert.ok(Number.isInteger(seconds) && seconds >= 10 && seconds <= 1800);
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts()[0].pages().find(p => p.url().includes('tauri'));
assert.ok(page, 'real native WebView');
const invoke = (command, args) => page.evaluate(({ command, args }) =>
  window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
try {
  const devices = await invoke('list_devices');
  const input = devices.find(d => d.input && d.id === process.env.MIC_TEST_INPUT_ID);
  const output = devices.find(d => !d.input && d.name.includes('CABLE Input'));
  assert.ok(input && output && !input.name.includes('CABLE'), 'explicit physical microphone and cable');
  const { session } = await invoke('load_session');
  await invoke('start_audio', { settings: { ...session, inputId: input.id, outputId: output.id,
    bufferMs: 5, monitoring: false, chain: [
      { id: 'soak-frequency', type: 'frequency', enabled: true, params: { shift_hz: 100 } },
      { id: 'soak-gain', type: 'gain', enabled: true, params: { gain_db: -6 } },
    ] } });
  console.log(JSON.stringify({ phase: 'started', seconds, input: input.name, bufferMs: 5, at: new Date().toISOString() }));
  let baseline;
  for (let elapsed = 1; elapsed <= seconds; elapsed++) {
    await new Promise(resolve => setTimeout(resolve, 1000));
    const status = await invoke('get_status');
    assert.ok(status.running && !status.error, JSON.stringify(status));
    assert.equal(status.overflows, 0, 'no dropped samples');
    if (elapsed === 10) baseline = status;
    if (baseline) {
      assert.equal(status.underruns, baseline.underruns, 'no new starvation after warmup');
      assert.equal(status.deviceXruns, baseline.deviceXruns, 'no new driver glitches after warmup');
    }
    if (elapsed % 10 === 0 || elapsed === seconds) {
      console.log(JSON.stringify({ elapsed, at: new Date().toISOString(), status }));
    }
  }
  console.log(JSON.stringify({ phase: 'complete', seconds, at: new Date().toISOString() }));
} finally {
  await invoke('stop_audio');
  await browser.close();
}
