import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AudioLines, Play, Square, Settings, ChevronDown, X, RefreshCw, Save, CheckCircle2, AlertCircle, Cable, Trash2, Plus } from 'lucide-react';
import * as api from './api';
import { definitions, builtins, emptySession, insertFilter, moveFilter, duplicateFilter, removeFilter, cloneChain, type Session, type FilterInstance, type FilterDefinition } from './model';
import { ChainEditor, type CardAction } from './ChainEditor';
import { FilterIcon, Menu, ParameterControl, WindowControls } from './ui';
import { version } from '../package.json';

function Meter({ label, value }: { label: string; value: number }) {
  return <div className="meter"><div><span>{label}</span><span>{value > 0 ? (20 * Math.log10(value)).toFixed(1) + ' dB' : '−∞ dB'}</span></div>
    <div className="meter-track"><span style={{ width: Math.min(100, Math.max(0, (20 * Math.log10(Math.max(value, .001)) + 60) / 60 * 100)) + '%' }} /></div></div>;
}

export default function App() {
  const [session, setSession] = useState<Session>({ ...emptySession });
  const [devices, setDevices] = useState<api.Device[]>([]);
  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState<api.Status>(api.stopped);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [announcement, setAnnouncement] = useState('');
  const [saveState, setSaveState] = useState('正在载入配置');
  const [presetName, setPresetName] = useState('');
  const settings = useRef<HTMLDialogElement>(null);
  const presetDialog = useRef<HTMLDialogElement>(null);
  const selected = session.chain.find(node => node.id === selectedId) ?? session.chain[0] ?? null;
  const definition = selected ? definitions.find(d => d.type === selected.type)! : null;

  useEffect(() => {
    let active = true;
    Promise.all([api.load(), api.devices()]).then(([saved, list]) => {
      if (!active) return;
      const restored = { ...saved.session };
      if (!restored.inputId) restored.inputId = list.find(d => d.input && d.isDefault && !d.name.includes('CABLE'))?.id ?? list.find(d => d.input && !d.name.includes('CABLE'))?.id ?? '';
      if (!restored.outputId) restored.outputId = list.find(d => !d.input && d.name.includes('CABLE Input'))?.id ?? '';
      if (!restored.monitorId) restored.monitorId = list.find(d => !d.input && d.isDefault && !d.name.includes('CABLE'))?.id ?? '';
      setDevices(list); setSession(restored); setSelectedId(restored.chain[0]?.id ?? null);
      setNotice(saved.warning ?? ''); setReady(true);
    }).catch(error => { if (active) setError(String(error)); });
    return () => { active = false; };
  }, []);
  useEffect(() => {
    if (!ready) return;
    let active = true;
    setSaveState('正在保存');
    const timer = setTimeout(() => api.save(session).then(() => { if (active) setSaveState('配置已保存'); })
      .catch(error => { if (active) { setSaveState('保存失败'); setError('保存失败：' + String(error)); } }), 350);
    return () => { active = false; clearTimeout(timer); };
  }, [session, ready]);
  useEffect(() => {
    if (!ready) return;
    const timer = setTimeout(() => api.chain(session.chain).catch(error => setError('滤镜更新失败：' + String(error))), 25);
    return () => clearTimeout(timer);
  }, [session.chain, ready]);
  useEffect(() => {
    if (!api.native) return;
    let active = true;
    const timer = setInterval(() => api.status().then(next => {
      if (!active) return;
      setStatus(next);
      if (next.running) setSession(previous => previous.monitoring === next.monitoring ? previous : { ...previous, monitoring: next.monitoring });
    }).catch(error => { if (active) setError(String(error)); }), 200);
    return () => { active = false; clearInterval(timer); };
  }, []);

  const select = useCallback((id: string | null, focus = false) => {
    setSelectedId(id);
    if (id && focus) requestAnimationFrame(() => document.getElementById('select-' + id)?.focus({ preventScroll: true }));
  }, []);
  const changeChain = useCallback((chain: FilterInstance[]) => {
    setError('');
    setSession(previous => ({ ...previous, chain }));
    setAnnouncement('声音管线已更新，共 ' + chain.length + ' 个滤镜。');
  }, []);
  const add = useCallback((definition: FilterDefinition, index: number) => {
    if (session.chain.length >= 64) { setNotice('最多可以加入 64 个滤镜。'); return; }
    const chain = insertFilter(session.chain, definition, index);
    changeChain(chain);
    select(chain[Math.max(0, Math.min(index, chain.length - 1))].id, true);
  }, [session.chain, changeChain, select]);
  const action = useCallback((id: string, action: CardAction) => {
    const index = session.chain.findIndex(node => node.id === id);
    if (index < 0) return;
    let chain = session.chain;
    if (action === 'bypass') chain = chain.map(node => node.id === id ? { ...node, enabled: !node.enabled } : node);
    else if (action === 'duplicate') {
      if (chain.length >= 64) { setNotice('最多可以加入 64 个滤镜。'); return; }
      chain = duplicateFilter(chain, id);
      select(chain[index + 1].id, true);
    } else if (action === 'delete') {
      chain = removeFilter(chain, id);
      if (selected?.id === id) select(chain[Math.min(index, chain.length - 1)]?.id ?? null, true);
    } else {
      chain = moveFilter(chain, id, index + (action === 'previous' ? -1 : 1));
      select(id, true);
    }
    changeChain(chain);
  }, [session.chain, selected?.id, changeChain, select]);
  const applyPreset = (chain: FilterInstance[]) => {
    const next = cloneChain(chain);
    changeChain(next);
    select(next[0]?.id ?? null);
  };
  const presetLabel = useMemo(() => {
    const matches = [...builtins, ...session.presets].find(p => p.chain.length === session.chain.length && p.chain.every((node, index) => {
      const other = session.chain[index];
      return node.type === other.type && node.enabled === other.enabled && Object.keys(node.params).every(key => node.params[key] === other.params[key]);
    }));
    return matches?.name ?? '自定义';
  }, [session.chain, session.presets]);
  const toggleAudio = async () => {
    setBusy(true); setError('');
    try { if (status.running) await api.stop(); else await api.start(session); setStatus(await api.status()); }
    catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  };
  const toggleMonitor = async (enabled: boolean) => {
    setError('');
    try { if (status.running) await api.monitoring(enabled); setSession(previous => ({ ...previous, monitoring: enabled })); }
    catch (error) { setError(String(error)); }
  };
  const refresh = async () => {
    try { setDevices(await api.devices()); } catch (error) { setError(String(error)); }
  };
  const install = async () => {
    setBusy(true);
    try { setNotice(await api.installCable()); } catch (error) { setError(String(error)); } finally { setBusy(false); }
  };
  const savePreset = () => {
    if (!presetName.trim() || session.presets.length >= 100) return;
    setSession(previous => ({ ...previous, presets: [...previous.presets, { id: crypto.randomUUID(), name: presetName.trim(), chain: cloneChain(previous.chain) }] }));
    setPresetName('');
    presetDialog.current?.close();
  };
  const inputs = devices.filter(d => d.input);
  const outputs = devices.filter(d => !d.input);
  const inputName = devices.find(d => d.id === session.inputId)?.name ?? '选择麦克风';
  const outputName = devices.find(d => d.id === session.outputId)?.name ?? '选择输出';
  const cablePresent = outputs.some(d => d.name.includes('CABLE Input'));
  const deviceSelect = (key: 'inputId' | 'outputId' | 'monitorId', label: string, list: api.Device[], placeholder: string) =>
    <label className="device-label">{label}<select aria-label={label} value={session[key]} disabled={status.running || !api.native}
      onChange={event => setSession(previous => ({ ...previous, [key]: event.target.value }))}>
      <option value="">{placeholder}</option>
      {session[key] && !list.some(d => d.id === session[key]) ? <option value={session[key]}>已保存设备不可用，请重新选择</option> : null}
      {list.map(d => <option key={d.id} value={d.id}>{d.name}</option>)}
    </select></label>;

  return <div className="app-shell">
    <header className="titlebar glass">
      <div className="titlebar-grab" data-tauri-drag-region><AudioLines size={25} strokeWidth={1.7} aria-hidden="true" /><h1>isMyMicThatBad</h1></div>
      <Menu label="声音预设" content={<><span>{presetLabel}</span><ChevronDown size={14} aria-hidden="true" /></>} className="preset-trigger" width={248} disabled={dragging}>
        <span className="menu-heading">声音预设</span>
        {builtins.map(p => <button key={p.id} role="menuitem" onClick={() => applyPreset(p.chain)}>{p.name}<small>{p.chain.length === 0 ? '直接输出' : p.chain.length + ' 个滤镜'}</small></button>)}
        {session.presets.length ? <span className="menu-heading">我的预设</span> : null}
        {session.presets.map(p => <button key={p.id} role="menuitem" onClick={() => applyPreset(p.chain)}>{p.name}<small>{p.chain.length} 个滤镜</small></button>)}
        <div className="menu-divider" /><button role="menuitem" disabled={session.presets.length >= 100} onClick={() => presetDialog.current?.showModal()}><Save size={16} aria-hidden="true" />保存当前管线</button>
      </Menu>
      <button className="route-summary" title={inputName + ' → ' + outputName} aria-label="打开音频路由设置" disabled={dragging} onClick={() => settings.current?.showModal()}>
        <span>{inputName.startsWith('Microphone') ? '麦克风' : inputName}</span><span aria-hidden="true">→</span><span>{outputName.startsWith('CABLE Input') ? 'CABLE Input' : outputName}</span>
      </button>
      <span className="status-label">{status.running ? '正在处理' : '已停止'}</span>
      <button className="transport" disabled={busy || !ready || !api.native || (!status.running && (!session.inputId || !session.outputId))} onClick={toggleAudio}>
        {status.running ? <Square size={15} aria-hidden="true" /> : <Play size={16} fill="currentColor" aria-hidden="true" />}
        {busy ? '请稍候' : status.running ? '停止处理' : '开始处理'}
      </button>
      <button className="settings-button icon-button" aria-label="打开设置" title="设备、监听与缓冲" disabled={dragging} onClick={() => settings.current?.showModal()}><Settings size={21} strokeWidth={1.7} aria-hidden="true" /></button>
      <WindowControls onError={setError} />
    </header>
    {!api.native ? <div className="preview-notice"><span>编辑器预览</span>真实音频请使用桌面应用</div> : null}
    {error || status.error ? <div role="alert" className="message-banner error-banner"><AlertCircle size={17} aria-hidden="true" /><span>{error || status.error}</span>
      {!ready ? <button onClick={() => window.location.reload()}>重新载入</button> : error ? <button aria-label="关闭错误提示" onClick={() => setError('')}><X size={15} aria-hidden="true" /></button> : null}</div> : null}
    {notice ? <div role="status" className="message-banner"><span>{notice}</span><button aria-label="关闭提示" onClick={() => setNotice('')}><X size={15} aria-hidden="true" /></button></div> : null}
    <main className="workspace">
      <ChainEditor chain={session.chain} selectedId={selected?.id ?? null} onSelect={select} onChange={changeChain} onAdd={add} onAction={action} onDraggingChange={setDragging} />
      <section className="inspector glass" aria-label="滤镜参数">
        {selected && definition ? <>
          <div className="inspector-identity"><div className="inspector-icon glass"><FilterIcon type={selected.type} size={34} /></div>
            <div><h2>{definition.name}</h2><span>{selected.enabled ? '参数' : '已旁路 · 参数保留'}</span></div>
          </div>
          <fieldset className={'inspector-fields' + (definition.parameters.length > 1 ? ' multiple-parameters' : '')} disabled={dragging}><legend className="sr-only">{definition.name}参数</legend>
            {definition.parameters.map(parameter => <ParameterControl key={selected.id + parameter.key} node={selected} parameter={parameter}
              onChange={(key, value) => changeChain(session.chain.map(node => node.id === selected.id ? { ...node, params: { ...node.params, [key]: value } } : node))} />)}
            <p className="inspector-description">{definition.description}</p>
          </fieldset>
        </> : <div className="inspector-empty"><FilterIcon type="custom" size={30} /><div><h2>选择一个滤镜</h2><p>参数会在这里显示</p></div></div>}
      </section>
    </main>
    <footer className="app-footer"><span><CheckCircle2 size={16} aria-hidden="true" />{saveState}</span>
      <span className="footer-health" title={'欠载 ' + status.underruns + '，溢出 ' + status.overflows + '，设备中断 ' + status.deviceXruns}>
        {status.underruns || status.overflows || status.deviceXruns ? <>欠载 {status.underruns} · 溢出 {status.overflows} · 设备中断 {status.deviceXruns}</> : null}
      </span><span className="footer-meta">48 kHz<span>v{version}</span></span>
    </footer>
    <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{announcement}</div>

    <dialog ref={settings} className="settings-dialog glass" aria-labelledby="settings-title">
      <div className="dialog-heading"><h2 id="settings-title">设置</h2><button className="icon-button" aria-label="关闭设置" onClick={() => settings.current?.close()}><X size={20} aria-hidden="true" /></button></div>
      <section className="settings-section"><div className="section-heading"><h3>音频路由</h3><button className="icon-button" aria-label="刷新音频设备" disabled={status.running} onClick={refresh}><RefreshCw size={17} aria-hidden="true" /></button></div>
        {deviceSelect('inputId', '麦克风输入', inputs, '选择真实麦克风')}
        {deviceSelect('outputId', '处理后输出', outputs, '选择 CABLE Input')}
        <p className="hint">聊天软件的麦克风选择 CABLE Output。更换设备前请先停止处理。</p>
      </section>
      <section className="settings-section"><h3>监听与缓冲</h3>
        {deviceSelect('monitorId', '监听设备', outputs.filter(d => d.id !== session.outputId), '选择耳机')}
        <label className="monitor-switch"><span>耳机监听</span><input type="checkbox" checked={session.monitoring} disabled={!session.monitorId || !api.native || busy} onChange={event => toggleMonitor(event.target.checked)} /></label>
        <label className="buffer-control">缓冲目标 <span>{session.bufferMs} ms</span><input aria-label="缓冲目标" disabled={status.running} type="range" min={5} max={30} step={1} value={session.bufferMs} onChange={event => setSession(previous => ({ ...previous, bufferMs: Number(event.target.value) }))} /></label>
        <p className="hint">发生断音时提高缓冲。端到端延迟以回环测量为准。</p>
        <Meter label="输入电平" value={status.inputPeak} /><Meter label="输出电平" value={status.outputPeak} />
      </section>
      {session.presets.length ? <section className="settings-section"><h3>我的预设</h3>{session.presets.map(p => <div className="saved-preset" key={p.id}><span>{p.name}</span><button className="icon-button" aria-label={'删除预设' + p.name} onClick={() => setSession(previous => ({ ...previous, presets: previous.presets.filter(item => item.id !== p.id) }))}><Trash2 size={16} aria-hidden="true" /></button></div>)}</section> : null}
      {api.native && !cablePresent ? <section className="settings-section"><h3><Cable size={17} aria-hidden="true" />连接到其他应用</h3><p className="hint">尚未检测到 VB-CABLE。完成安装后重启 Windows。</p>
        <button className="secondary-button" disabled={busy} onClick={install}>安装随包附带的 VB-CABLE</button>
        <a href="https://vb-audio.com/Cable/" onClick={event => { if (api.native) { event.preventDefault(); api.openCableSite().catch(error => setError(String(error))); } }} target="_blank" rel="noreferrer">VB-Audio 官方来源与捐赠</a>
      </section> : null}
    </dialog>
    <dialog ref={presetDialog} className="save-dialog glass" aria-labelledby="save-preset-title">
      <div className="dialog-heading"><h2 id="save-preset-title">保存当前管线</h2><button className="icon-button" aria-label="取消保存预设" onClick={() => presetDialog.current?.close()}><X size={19} aria-hidden="true" /></button></div>
      <form onSubmit={event => { event.preventDefault(); savePreset(); }}>
        <label htmlFor="preset-name">预设名称</label><input id="preset-name" placeholder="给声音起个名字" maxLength={80} value={presetName} onChange={event => setPresetName(event.target.value)} autoFocus />
        <button className="transport" type="submit" aria-label="保存预设" disabled={!presetName.trim() || session.presets.length >= 100}><Plus size={16} aria-hidden="true" />保存预设</button>
      </form>
    </dialog>
  </div>;
}
