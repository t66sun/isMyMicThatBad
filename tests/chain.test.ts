import test from 'node:test';
import assert from 'node:assert/strict';
import { createFilter, emptySession, insertFilter, moveFilter, duplicateFilter, removeFilter, validateSession, definitions, builtins } from '../src/app/src/model.ts';
import { editChain, undoChain, redoChain, emptyHistory } from '../src/app/src/history.ts';
test('insertion, reorder and duplication preserve independent instance identity', () => {
  let chain = insertFilter([], definitions[0], 0);
  chain = insertFilter(chain, definitions[1], 0);
  assert.deepEqual(chain.map(x => x.type), ['highpass', 'frequency']);
  const id = chain[0].id;
  chain = moveFilter(chain, id, 1);
  assert.equal(chain[1].id, id);
  chain = duplicateFilter(chain, id);
  assert.equal(chain.length, 3);
  assert.notEqual(chain[1].id, chain[2].id);
  chain[2].params.cutoff = 500;
  assert.equal(chain[1].params.cutoff, 80);
  assert.equal(removeFilter(chain, id).length, 2);
});
test('all thirteen filters and seven presets have complete valid independent metadata', () => {
  assert.equal(definitions.length, 13); assert.equal(builtins.length, 7);
  assert.equal(new Set(definitions.map(d => d.type)).size, definitions.length);
  for (const definition of definitions) {
    const node = createFilter(definition);
    assert.deepEqual(validateSession({...emptySession, chain:[node]}).chain, [node]);
    for (const key of definition.summaryKeys ?? []) assert.ok(definition.parameters.some(p => p.key === key));
  }
  for (const preset of builtins) validateSession({...emptySession, chain:preset.chain});
  assert.deepEqual(builtins.slice(3).map(p => p.name), ['女声','炸麦','广播声','空灵声']);
});
test('bitcrusher exposes the declared bit-depth, hold and mix bounds', () => {
  const definition = definitions.find(d => d.type === 'bitcrusher')!;
  assert.deepEqual(definition.parameters.map(({key,min,max,step,default:value}) => ({key,min,max,step,default:value})), [
    {key:'bits',min:2,max:16,step:1,default:8},
    {key:'hold',min:1,max:48,step:1,default:4},
    {key:'mix',min:0,max:100,step:1,default:100},
  ]);
  const node = createFilter(definition);
  for (const [key,value] of [['bits',2],['bits',16],['hold',1],['hold',48],['mix',0],['mix',100]] as const) {
    assert.equal(validateSession({...emptySession,chain:[{...node,params:{...node.params,[key]:value}}]}).chain[0].params[key],value);
  }
  for (const [key,value] of [['bits',1],['bits',17],['hold',0],['hold',49],['mix',-1],['mix',101]] as const) {
    assert.throws(() => validateSession({...emptySession,chain:[{...node,params:{...node.params,[key]:value}}]}));
  }
});

test('chain history restores identities, parameters, order and bypass without mutating snapshots', () => {
  const initial = insertFilter([], definitions[0], 0);
  const duplicate = duplicateFilter(initial, initial[0].id);
  const edited = duplicate.map((node, i) => i ? { ...node, enabled: false, params: { shift_hz: 200 } } : node);
  const reordered = moveFilter(edited, edited[1].id, 0);
  const deleted = removeFilter(reordered, reordered[1].id);
  const snapshots = [initial, duplicate, edited, reordered, deleted, builtins[3].chain];
  let history = emptyHistory;
  for (const chain of snapshots) history = editChain(history, chain);
  assert.equal(editChain(history, structuredClone(history.present)), history, 'no-op does not create an edit');
  for (const chain of [...snapshots.slice(0, -1)].reverse().concat([[]])) {
    history = undoChain(history); assert.deepEqual(history.present, chain);
  }
  assert.equal(undoChain(history), history);
  for (const chain of snapshots) { history = redoChain(history); assert.deepEqual(history.present, chain); }
  assert.equal(redoChain(history), history);
  history = editChain(undoChain(history), initial);
  assert.equal(history.future.length, 0, 'editing after undo clears redo');
  assert.equal(initial[0].enabled, true); assert.equal(initial[0].params.shift_hz, 0);
});

test('one slider gesture is one undo and later gestures remain separate', () => {
  const initial = insertFilter([], definitions[0], 0);
  let history = { ...emptyHistory, present: initial };
  const change = (value: number) => initial.map(node => ({ ...node, params: { shift_hz: value } }));
  history = editChain(history, change(100), 'slider');
  history = editChain(history, change(200), 'slider');
  assert.equal(history.past.length, 1);
  assert.deepEqual(undoChain(history).present, initial);
  history = { ...history, group: null };
  history = editChain(history, change(300), 'slider');
  assert.equal(undoChain(history).present[0].params.shift_hz, 200);
  history = editChain(undoChain(history), change(400), 'slider');
  assert.equal(history.past.length, 2); assert.equal(history.future.length, 0);
});
test('legacy distortion normalization retains identity and defaults only missing clipping', () => {
  const old = {id:'old-distortion',type:'distortion',enabled:true,params:{drive_db:6,mix:25}};
  const original = JSON.stringify(old);
  const normalized = validateSession({...emptySession,chain:[old],presets:[{id:'old-preset',name:'旧预设',chain:[old]}]});
  assert.equal(normalized.chain[0].params.clip,0);
  assert.equal(normalized.presets[0].chain[0].params.clip,0);
  assert.equal(normalized.chain[0].id,old.id); assert.equal(JSON.stringify(old),original);
  for (const clip of [-1,101,NaN,Infinity,null]) assert.throws(()=>validateSession({...emptySession,chain:[{...old,params:{...old.params,clip}}]}));
});
test('new multi-parameter instances duplicate independently and reject incomplete values', () => {
  const node = createFilter(definitions.find(d=>d.type==='eq')!);
  const pair = duplicateFilter([node],node.id);
  pair[1].params.mid_q=2;assert.equal(pair[0].params.mid_q,1);
  assert.notEqual(pair[0].id,pair[1].id);
  const missing = {...node,params:{...node.params}}; delete missing.params.mid_q;
  assert.throws(()=>validateSession({...emptySession,chain:[missing]}));
});
test('saved configuration rejects invalid filter values and recovers valid presets', () => {
  const session = { schemaVersion: 1, inputId: '', outputId: '', monitorId: '', monitoring: false, bufferMs: 10, chain: builtins[1].chain, presets: [] };
  assert.equal(validateSession(session).chain.length, 3);
  assert.throws(() => validateSession({...session, chain: [{id:'x',type:'frequency',enabled:true,params:{shift_hz:Infinity}}]}));
  assert.throws(() => validateSession({...session, chain: [{id:'x',type:'unknown',enabled:true,params:{}}]}));
});

test('close-to-tray defaults off for legacy settings and validates persisted choices', () => {
  const {closeToTray, ...legacy} = emptySession;
  assert.equal(validateSession(legacy).closeToTray, false);
  for (const enabled of [false, true]) assert.equal(validateSession({...legacy, closeToTray:enabled}).closeToTray, enabled);
  for (const value of [null, 'true', 1]) assert.throws(() => validateSession({...legacy, closeToTray:value}));
});
