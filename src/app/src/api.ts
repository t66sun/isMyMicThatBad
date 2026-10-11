import { invoke } from '@tauri-apps/api/core';
import { emptySession, validateSession, type Session, type FilterInstance } from './model';
export interface Device { id: string; name: string; input: boolean; isDefault: boolean }
export interface Status { running: boolean; bypassed: boolean; monitoring: boolean; inputPeak: number; outputPeak: number; underruns: number; overflows: number; deviceXruns: number; error: string | null; sampleRate: number }
export const native = '__TAURI_INTERNALS__' in window;
export const stopped: Status = {running:false,bypassed:false,monitoring:false,inputPeak:0,outputPeak:0,underruns:0,overflows:0,deviceXruns:0,error:null,sampleRate:48000};
export async function load(): Promise<{session:Session; warning:string|null}> {
  if (native) { const data = await invoke<{session:Session; warning:string|null}>('load_session'); return {...data,session:validateSession(data.session)}; }
  try { const saved = localStorage.getItem('mic-preview'); return {session:saved ? validateSession(JSON.parse(saved)) : {...emptySession},warning:null}; }
  catch { return {session:{...emptySession},warning:'预览配置无效，已恢复默认。'}; }
}
export async function save(session: Session) {
  const normalized = validateSession(session);
  if (native) await invoke('save_session',{session:normalized}); else localStorage.setItem('mic-preview',JSON.stringify(normalized));
}
export async function devices(): Promise<Device[]> { return native ? invoke('list_devices') : []; }
export async function chain(filters: FilterInstance[]) { if (native) await invoke('set_chain',{filters}); }
export async function start(session: Session) { if (!native) throw new Error('浏览器只提供编辑器预览；真实音频请打开桌面应用。'); await invoke('start_audio',{settings:session}); }
export async function stop() { if (native) await invoke('stop_audio'); }
export async function monitoring(enabled:boolean) { if (native) await invoke('set_monitoring',{enabled}); }
export async function bypass(bypassed:boolean) { if (native) await invoke('set_bypassed',{bypassed}); }
export async function status(): Promise<Status> { return native ? invoke('get_status') : {...stopped}; }
export async function installCable(): Promise<string> { return invoke('install_cable'); }

export async function openCableSite() { if(native) await invoke('open_cable_site'); }
