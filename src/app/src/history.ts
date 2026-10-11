import type { FilterInstance } from './model';

export interface ChainHistory {
  past: FilterInstance[][];
  present: FilterInstance[];
  future: FilterInstance[][];
  group: string | null;
}
export const emptyHistory: ChainHistory = { past: [], present: [], future: [], group: null };

export function editChain(history: ChainHistory, chain: FilterInstance[], group: string | null = null): ChainHistory {
  if (JSON.stringify(chain) === JSON.stringify(history.present)) return history;
  return {
    // shortcut: keep the latest 100 edits in memory, expand if longer editing sessions need them.
    past: group !== null && group === history.group ? history.past : [...history.past.slice(-99), history.present],
    present: chain, future: [], group,
  };
}
export function undoChain(history: ChainHistory): ChainHistory {
  if (!history.past.length) return history;
  return { past: history.past.slice(0, -1), present: history.past.at(-1)!, future: [history.present, ...history.future], group: null };
}
export function redoChain(history: ChainHistory): ChainHistory {
  if (!history.future.length) return history;
  return { past: [...history.past, history.present], present: history.future[0], future: history.future.slice(1), group: null };
}
