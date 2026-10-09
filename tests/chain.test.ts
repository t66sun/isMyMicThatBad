import test from 'node:test';
import assert from 'node:assert/strict';
import { insertFilter, moveFilter, duplicateFilter, removeFilter, validateSession, definitions, builtins } from '../src/app/src/model.ts';
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
test('saved configuration rejects invalid filter values and recovers valid presets', () => {
  const session = { schemaVersion: 1, inputId: '', outputId: '', monitorId: '', monitoring: false, bufferMs: 10, chain: builtins[1].chain, presets: [] };
  assert.equal(validateSession(session).chain.length, 3);
  assert.throws(() => validateSession({...session, chain: [{id:'x',type:'frequency',enabled:true,params:{shift_hz:Infinity}}]}));
  assert.throws(() => validateSession({...session, chain: [{id:'x',type:'unknown',enabled:true,params:{}}]}));
});
