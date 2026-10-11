import assert from 'node:assert/strict';

export const stored = page => page.evaluate(async () => window.__TAURI_INTERNALS__
  ? (await window.__TAURI_INTERNALS__.invoke('load_session')).session
  : JSON.parse(localStorage.getItem('mic-preview')));
export const settle = page => page.waitForTimeout(550);
export const labels = page => page.locator('.effect-card').evaluateAll(nodes => nodes.map(n => n.getAttribute('aria-label')));
export async function raw(page) {
  await page.getByRole('button', { name: '声音预设', exact: true }).click();
  await page.getByRole('menuitem', { name: /^原音/ }).click();
  await settle(page);
}

export async function historyFlow(page) {
  await raw(page); await page.reload(); await settle(page);
  const undo = page.getByRole('button', { name: '撤销管线修改', exact: true });
  const redo = page.getByRole('button', { name: '重做管线修改', exact: true });
  assert.ok(await undo.isDisabled()); assert.ok(await redo.isDisabled());
  const snapshots = [(await stored(page)).chain];
  const remember = async () => { await settle(page); snapshots.push((await stored(page)).chain); };
  await page.getByRole('button', { name: '添加增益', exact: true }).click(); await remember();
  await page.getByRole('spinbutton', { name: '增益', exact: true }).fill('-6');
  await page.getByRole('spinbutton', { name: '增益', exact: true }).press('Enter'); await remember();
  await page.getByRole('button', { name: '旁路增益', exact: true }).click(); await remember();
  await menu(page, 0, '复制滤镜'); await remember();
  await menu(page, 1, '向前移动'); await remember();
  await menu(page, 0, '删除滤镜'); await remember();
  await page.getByRole('button', { name: '声音预设', exact: true }).click();
  await page.getByRole('menuitem', { name: /^女声/ }).click(); await remember();
  for (const chain of snapshots.slice(0, -1).reverse()) {
    await undo.click(); await settle(page); assert.deepEqual((await stored(page)).chain, chain);
  }
  assert.ok(await undo.isDisabled());
  await page.locator('.chain-viewport').focus(); await page.keyboard.press('Control+Shift+Z'); await settle(page);
  assert.deepEqual((await stored(page)).chain, snapshots[1]);
  await page.keyboard.press('Control+Y'); await settle(page);
  assert.deepEqual((await stored(page)).chain, snapshots[2]);
  const input = page.getByRole('spinbutton', { name: '增益', exact: true });
  await input.focus(); await page.keyboard.press('Control+Z'); await settle(page);
  assert.deepEqual((await stored(page)).chain, snapshots[2], 'text inputs retain their own undo');
  await page.locator('.chain-viewport').focus(); await page.keyboard.press('Control+Z'); await settle(page);
  assert.deepEqual((await stored(page)).chain, snapshots[1]);
  await redo.click(); await settle(page);
  const slider = page.getByRole('slider', { name: '增益滑块', exact: true });
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down(); await page.mouse.move(box.x + box.width * .7, box.y + box.height / 2, { steps: 12 });
  await page.mouse.up(); await settle(page);
  assert.notDeepEqual((await stored(page)).chain, snapshots[2]);
  await undo.click(); await settle(page);
  assert.deepEqual((await stored(page)).chain, snapshots[2], 'whole slider gesture undoes in one step');
  await page.getByRole('button', { name: '添加颤音', exact: true }).click(); await settle(page);
  assert.ok(await redo.isDisabled(), 'new edit clears redo');
  await page.getByRole('button', { name: '添加数字降质', exact: true }).click(); await settle(page);
  const bitcrusher = page.getByRole('spinbutton', { name: '位深', exact: true });
  assert.deepEqual((await stored(page)).chain.at(-1).params, { bits: 8, hold: 4, mix: 100 });
  await bitcrusher.fill('12'); await bitcrusher.press('Enter'); await settle(page);
  assert.equal((await stored(page)).chain.at(-1).params.bits, 12);
  await undo.click(); await settle(page);
  assert.equal((await stored(page)).chain.at(-1).params.bits, 8, 'bitcrusher parameter edit is undoable');
  await redo.click(); await settle(page);
  assert.equal((await stored(page)).chain.at(-1).params.bits, 12, 'bitcrusher parameter edit is redoable');
  await menu(page, (await stored(page)).chain.length - 1, '复制滤镜');
  await page.getByRole('button', { name: '选择数字降质滤镜', exact: true }).last().click();
  const hold = page.getByRole('spinbutton', { name: '采样保持', exact: true });
  await hold.fill('9'); await hold.press('Enter'); await settle(page);
  const mix = page.getByRole('spinbutton', { name: '混合', exact: true });
  await mix.fill('75'); await mix.press('Enter'); await settle(page);
  const bitcrushers = (await stored(page)).chain.slice(-2);
  assert.notEqual(bitcrushers[0].id, bitcrushers[1].id);
  assert.deepEqual(bitcrushers.map(node => node.params), [
    { bits: 12, hold: 4, mix: 100 },
    { bits: 12, hold: 9, mix: 75 },
  ], 'bitcrusher copies keep independent parameters');
  const changed = (await stored(page)).chain;
  await page.reload(); await settle(page);
  assert.deepEqual((await stored(page)).chain, changed);
  assert.ok(await undo.isDisabled()); assert.ok(await redo.isDisabled(), 'history is session-local');
}
export async function beginDrag(page, locator) {
  const box = await locator.boundingBox();
  assert.ok(box);
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 9, box.y + box.height / 2, { steps: 3 });
  await page.locator('.overlay-card').waitFor();
}
export async function movePointer(page, x, y) {
  await page.mouse.move(x, y, { steps: 24 });
  await page.waitForTimeout(220);
}
export async function finishDrag(page) {
  await page.mouse.up();
  await page.locator('.overlay-card').waitFor({ state: 'hidden' });
  await settle(page);
}
async function menu(page, index, action) {
  await page.locator('.effect-card').nth(index).getByRole('button', { name: /更多操作/ }).click();
  await page.getByRole('menuitem', { name: action, exact: true }).click();
  await settle(page);
}
export async function editorFlow(page) {
  await page.getByRole('heading', { name: '声音管线', exact: true }).waitFor();
  const compare = page.getByRole('button', { name: '原音对比', exact: true });
  assert.ok(await compare.isDisabled());
  assert.equal(await compare.getAttribute('aria-pressed'), 'false');
  await raw(page);
  assert.equal(await page.locator('.inspector-empty').count(), 1);
  const chainBox = await page.locator('.chain-viewport').boundingBox();
  await beginDrag(page, page.getByRole('button', { name: '添加频率平移', exact: true }));
  await movePointer(page, chainBox.x + chainBox.width / 2, chainBox.y + chainBox.height / 2);
  await page.locator('.library-placeholder').waitFor();
  await settle(page);
  assert.equal((await stored(page)).chain.length, 0, 'library drag is only a preview');
  await finishDrag(page);
  assert.deepEqual(await labels(page), ['频率平移滤镜']);
  for (const name of ['高通', '增益']) await page.getByRole('button', { name: '添加' + name, exact: true }).click();
  await settle(page);
  const before = (await stored(page)).chain;
  const gain = await page.locator('.effect-card').last().boundingBox();
  await beginDrag(page, page.getByRole('button', { name: '选择频率平移滤镜', exact: true }));
  await movePointer(page, gain.x + gain.width - 8, gain.y + gain.height / 2);
  await settle(page);
  assert.deepEqual((await stored(page)).chain, before, 'reorder is uncommitted until release');
  await finishDrag(page);
  assert.deepEqual(await labels(page), ['高通滤镜', '增益滤镜', '频率平移滤镜']);
  assert.deepEqual((await stored(page)).chain.map(n => n.id).sort(), before.map(n => n.id).sort());
  const reordered = (await stored(page)).chain;
  const first = await page.locator('.effect-card').first().boundingBox();
  await beginDrag(page, page.getByRole('button', { name: '选择频率平移滤镜', exact: true }));
  await movePointer(page, first.x + 10, first.y + first.height / 2);
  await page.keyboard.press('Escape'); await finishDrag(page);
  assert.deepEqual((await stored(page)).chain, reordered, 'Escape restores original chain');
  assert.deepEqual(await labels(page), ['高通滤镜', '增益滤镜', '频率平移滤镜']);
  await beginDrag(page, page.getByRole('button', { name: '添加低通', exact: true }));
  await movePointer(page, 40, 38); await finishDrag(page);
  assert.deepEqual((await stored(page)).chain, reordered, 'outside drop cancels insertion');
  await page.getByRole('button', { name: '选择频率平移滤镜', exact: true }).focus();
  await page.keyboard.press('Space');
  assert.equal(await page.locator('.overlay-card').count(), 0, 'Space selects without entering drag mode');
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).waitFor();
  await page.keyboard.press('Alt+ArrowLeft'); await settle(page);
  assert.deepEqual(await labels(page), ['高通滤镜', '频率平移滤镜', '增益滤镜']);
  await page.keyboard.press('Alt+ArrowUp'); await settle(page);
  assert.equal((await labels(page))[0], '频率平移滤镜');
  await menu(page, 0, '向后移动');
  assert.deepEqual(await labels(page), ['高通滤镜', '频率平移滤镜', '增益滤镜']);
  await menu(page, 1, '向前移动');
  const middleBefore = (await stored(page)).chain;
  const middleBox = await page.locator('.effect-card').first().boundingBox();
  await beginDrag(page, page.getByRole('button', { name: '添加低通', exact: true }));
  await movePointer(page, middleBox.x + middleBox.width + 12, middleBox.y + middleBox.height / 2);
  await settle(page);
  assert.deepEqual((await stored(page)).chain, middleBefore);
  await finishDrag(page);
  assert.deepEqual(await labels(page), ['频率平移滤镜', '低通滤镜', '高通滤镜', '增益滤镜']);
  await menu(page, 1, '删除滤镜');
  await page.getByRole('button', { name: '在位置 1 插入滤镜', exact: true }).click();
  await page.getByRole('button', { name: '添加低通', exact: true }).click();
  assert.equal((await labels(page))[0], '低通滤镜');
  await menu(page, 0, '删除滤镜');
  await page.getByRole('button', { name: '选择频率平移滤镜', exact: true }).click();
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).fill('100');
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).press('Enter');
  await menu(page, 0, '复制滤镜');
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).fill('222');
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).press('Enter');
  await settle(page);
  const copies = (await stored(page)).chain;
  assert.equal(copies[0].params.shift_hz, 100); assert.equal(copies[1].params.shift_hz, 222);
  assert.notEqual(copies[0].id, copies[1].id);
  await menu(page, 1, '删除滤镜');
  await page.getByRole('spinbutton', { name: '截止频率', exact: true }).waitFor();
  await page.getByRole('button', { name: '旁路频率平移', exact: true }).click(); await settle(page);
  assert.equal((await stored(page)).chain[0].enabled, false);
  await page.getByRole('button', { name: '启用频率平移', exact: true }).click();
  const presetName = '界面验证-' + Date.now();
  await page.getByRole('button', { name: '声音预设', exact: true }).click();
  await page.getByRole('menuitem', { name: '保存当前管线', exact: true }).click();
  await page.getByRole('textbox', { name: '预设名称', exact: true }).fill(presetName);
  await page.getByRole('button', { name: '保存预设', exact: true }).click(); await settle(page);
  assert.ok((await stored(page)).presets.some(p => p.name === presetName));
  const saved = (await stored(page)).chain;
  await page.reload(); await page.locator('.effect-card').first().waitFor();
  assert.deepEqual((await stored(page)).chain, saved);
  assert.equal(await page.getByRole('spinbutton', { name: '平移量', exact: true }).inputValue(), '100');
  await raw(page);
  for (let i = 0; i < 20; i++) await page.getByRole('button', { name: '添加增益', exact: true }).click();
  await settle(page);
  await page.locator('.chain-viewport').hover(); await page.mouse.wheel(0, -6000);
  await page.waitForFunction(() => document.querySelector('.chain-viewport').scrollLeft < 2);
  await page.mouse.wheel(0, 500);
  await page.waitForFunction(() => document.querySelector('.chain-viewport').scrollLeft > 100);
  await page.mouse.wheel(0, -6000);
  await page.waitForFunction(() => document.querySelector('.chain-viewport').scrollLeft < 2);
  await beginDrag(page, page.getByRole('button', { name: '选择增益滤镜', exact: true }).first());
  await movePointer(page, chainBox.x + chainBox.width - 8, chainBox.y + chainBox.height / 2);
  await page.waitForFunction(() => document.querySelector('.chain-viewport').scrollLeft > 100);
  await page.keyboard.press('Escape'); await finishDrag(page);
  assert.equal((await stored(page)).chain.length, 20);
  await raw(page);
  for (const name of ['频率平移', '高通', '增益', '低通', '失真']) await page.getByRole('button', { name: '添加' + name, exact: true }).click();
  await page.getByRole('button', { name: '选择失真滤镜', exact: true }).click();
  await page.getByRole('spinbutton', { name: '驱动', exact: true }).fill('200');
  await page.getByRole('spinbutton', { name: '驱动', exact: true }).press('Enter');
  assert.equal(await page.getByRole('spinbutton', { name: '驱动', exact: true }).inputValue(), '24');
  await page.getByRole('spinbutton', { name: '混合', exact: true }).fill('40');
  await page.getByRole('spinbutton', { name: '混合', exact: true }).press('Enter'); await settle(page);
  assert.deepEqual((await stored(page)).chain[4].params, { drive_db: 24, mix: 40, clip: 0 });
  for (const [name, value] of [['驱动', '6'], ['混合', '25']]) {
    await page.getByRole('spinbutton', { name, exact: true }).fill(value);
    await page.getByRole('spinbutton', { name, exact: true }).press('Enter');
  }
  await page.getByRole('button', { name: '选择频率平移滤镜', exact: true }).click();
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).fill('100');
  await page.getByRole('spinbutton', { name: '平移量', exact: true }).press('Enter'); await settle(page);
  return { presetName, checks: 'drag preview/commit/cancel, outside drop, keyboard, insertion, copy/delete, bypass, preset/reload, wheel/edge scroll' };
}
