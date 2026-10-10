import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { AudioLines, SlidersHorizontal, Minus, Square, Copy, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { native } from './api';
import type { FilterDefinition, FilterInstance, Parameter } from './model';

export function FilterIcon({ type, size = 26 }: { type: string; size?: number }) {
  const paths: Record<string, string> = {
    frequency: 'M2 13c3-12 7-12 10 0s7 12 10 0',
    highpass: 'M3 21 12 4h9',
    lowpass: 'M3 4h9l9 17',
    distortion: 'M2 17c3 0 4-12 9-12s4 12 11 12',
    gate: 'M3 17h5V7h8v10h5M8 4v16M16 4v16',
    formant: 'M2 17c3 0 3-9 6-9s3 9 6 9 3-13 8-13',
    eq: 'M4 3v18M12 3v18M20 3v18M1 8h6M9 16h6M17 6h6',
    compressor: 'M3 19 11 11 21 7M3 4v17h18',
    pitch: 'M5 18V6m-3 3 3-3 3 3M16 6v12m-3-3 3 3 3-3',
    reverb: 'M2 10v4M6 6v12M10 3v18M14 6v12M18 9v6M22 11v2',
  };
  if (type === 'gain') return <AudioLines size={size} strokeWidth={1.7} aria-hidden="true" />;
  if (!paths[type]) return <SlidersHorizontal size={size} strokeWidth={1.7} aria-hidden="true" />;
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[type]} /></svg>;
}

export function useReducedMotion() {
  const [reduced, setReduced] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduced(media.matches);
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  return reduced;
}

export function Summary({ node, definition }: { node: FilterInstance; definition: FilterDefinition }) {
  const parameters = definition.summaryKeys ? definition.summaryKeys.map(key => definition.parameters.find(p => p.key === key)!) : definition.parameters;
  return <div className="card-values">{parameters.slice(0, 2).map(p =>
    <span key={p.key}>{definition.summaryKeys && parameters.length > 1 ? <small>{p.label}</small> : null}
      {node.params[p.key].toLocaleString('zh-CN', { maximumFractionDigits: 2 })}{!p.unit ? '' : ['%', ':1'].includes(p.unit) ? p.unit : ' ' + p.unit}</span>
  )}</div>;
}

export function ParameterControl({ node, parameter, onChange }: {
  node: FilterInstance; parameter: Parameter; onChange: (key: string, value: number) => void;
}) {
  const value = node.params[parameter.key];
  const [draft, setDraft] = useState(String(value));
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { if (document.activeElement !== input.current) setDraft(String(value)); }, [value]);
  const commit = () => {
    const parsed = Number(draft);
    const next = Number.isFinite(parsed) && draft.trim() ? Math.max(parameter.min, Math.min(parameter.max, parsed)) : value;
    setDraft(String(next));
    onChange(parameter.key, next);
  };
  return <div className="parameter">
    <label htmlFor={node.id + parameter.key}>{parameter.label}</label>
    <div className="parameter-controls">
      <div className="slider-group">
        <input aria-label={parameter.label + '滑块'} type="range" min={parameter.min} max={parameter.max} step={parameter.step} value={value}
          onChange={e => { const next = Number(e.target.value); setDraft(String(next)); onChange(parameter.key, next); }} />
        <div className="range-label"><span>{parameter.min.toLocaleString()}</span><span>{parameter.max > 0 ? '+' : ''}{parameter.max.toLocaleString()}</span></div>
      </div>
      <div className="number-value">
        <input ref={input} id={node.id + parameter.key} type="number" min={parameter.min} max={parameter.max} step={parameter.step} value={draft}
          onChange={e => setDraft(e.target.value)} onBlur={commit} onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur(); }} />
        <span>{parameter.unit}</span>
      </div>
    </div>
  </div>;
}

export function Menu({ label, children, content, className = '', width = 220, disabled = false }: {
  label: string; children: ReactNode; content: ReactNode; className?: string; width?: number; disabled?: boolean;
}) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const close = () => { panel.current?.hidePopover(); button.current?.focus({ preventScroll: true }); };
  return <>
    <button ref={button} className={className} disabled={disabled} aria-label={label} aria-haspopup="menu" aria-expanded={open} aria-controls={id}
      onClick={() => {
        if (!panel.current || !button.current) return;
        if (open) { close(); return; }
        const rect = button.current.getBoundingClientRect();
        panel.current.style.left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)) + 'px';
        panel.current.style.top = Math.min(rect.bottom + 8, window.innerHeight - Math.min(panel.current.scrollHeight || 250, 440) - 8) + 'px';
        panel.current.showPopover();
        panel.current.querySelector<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)')?.focus();
      }}>{content}</button>
    <div id={id} ref={panel} className="menu-panel glass" role="menu" aria-label={label} popover="auto" style={{ width }}
      onToggle={e => setOpen(e.newState === 'open')}
      onClick={e => { if ((e.target as Element).closest('[role="menuitem"]')) close(); }}
      onKeyDown={e => {
        const items = Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)') ?? []);
        const index = items.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === 'Escape') { e.preventDefault(); close(); }
        else if (e.key === 'Tab') panel.current?.hidePopover();
        else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
          e.preventDefault();
          const next = e.key === 'Home' ? 0 : e.key === 'End' ? items.length - 1 : (index + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
          items[next]?.focus();
        }
      }}>{children}</div>
  </>;
}

export function WindowControls({ onError }: { onError: (error: string) => void }) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!native) return;
    const window = getCurrentWindow();
    let active = true;
    let unlisten: (() => void) | undefined;
    const update = () => window.isMaximized().then(value => { if (active) setMaximized(value); }).catch(error => { if (active) onError(String(error)); });
    update();
    window.onResized(update).then(fn => { if (active) unlisten = fn; else fn(); }).catch(error => { if (active) onError(String(error)); });
    return () => { active = false; unlisten?.(); };
  }, [onError]);
  const act = (action: 'minimize' | 'toggleMaximize' | 'close') => {
    if (native) getCurrentWindow()[action]().catch(error => onError(String(error)));
  };
  return <div className="window-controls">
    <button aria-label="最小化窗口" disabled={!native} onClick={() => act('minimize')}><Minus size={16} aria-hidden="true" /></button>
    <button aria-label={maximized ? '还原窗口' : '最大化窗口'} disabled={!native} onClick={() => act('toggleMaximize')}>
      {maximized ? <Copy size={14} aria-hidden="true" /> : <Square size={14} aria-hidden="true" />}
    </button>
    <button className="window-close" aria-label="关闭窗口" disabled={!native} onClick={() => act('close')}><X size={17} aria-hidden="true" /></button>
  </div>;
}
