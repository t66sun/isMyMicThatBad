import definitionsJson from './filters.json';
export type FilterType = string;
export interface Parameter { key: string; label: string; unit: string; min: number; max: number; step: number; default: number }
export interface FilterDefinition { type: FilterType; name: string; description: string; color: string; parameters: Parameter[]; summaryKeys?: string[] }
export interface AiModel { path: string; index: string }
export interface FilterInstance { id: string; type: FilterType; enabled: boolean; params: Record<string, number>; model?: AiModel }
export interface Preset { id: string; name: string; chain: FilterInstance[] }
export interface Session { schemaVersion: 1; inputId: string; outputId: string; monitorId: string; monitoring: boolean; closeToTray: boolean; bufferMs: number; chain: FilterInstance[]; presets: Preset[] }
export const definitions = definitionsJson as FilterDefinition[];
export function createFilter(definition: FilterDefinition): FilterInstance {
  return {id: crypto.randomUUID(), type: definition.type, enabled: true, params: Object.fromEntries(definition.parameters.map(p => [p.key, p.default]))};
}
export function cloneChain(chain: FilterInstance[]): FilterInstance[] { return chain.map(x => ({...x, id: crypto.randomUUID(), params: {...x.params}, ...(x.model ? {model: {...x.model}} : {})})); }
export function insertFilter(chain: FilterInstance[], definition: FilterDefinition, index: number): FilterInstance[] {
  if (definition.type === 'rvc' && chain.some(x => x.type === 'rvc')) return chain;
  const next = [...chain]; next.splice(Math.max(0, Math.min(index, next.length)), 0, createFilter(definition)); return next;
}
export function moveFilter(chain: FilterInstance[], id: string, index: number): FilterInstance[] {
  const item = chain.find(x => x.id === id); if (!item) return chain;
  const next = chain.filter(x => x.id !== id); next.splice(Math.max(0, Math.min(index, next.length)), 0, item); return next;
}
export function duplicateFilter(chain: FilterInstance[], id: string): FilterInstance[] {
  const index = chain.findIndex(x => x.id === id); if (index < 0) return chain;
  if (chain[index].type === 'rvc') return chain;
  const next = [...chain]; next.splice(index + 1, 0, ...cloneChain([chain[index]])); return next;
}
export function removeFilter(chain: FilterInstance[], id: string): FilterInstance[] { return chain.filter(x => x.id !== id); }
const node = (type: FilterType, params: Record<string, number>, id: string): FilterInstance => {
  const definition = definitions.find(item => item.type === type)!;
  return {id, type, enabled: true, params: {...Object.fromEntries(definition.parameters.map(p => [p.key, p.default])), ...params}};
};
export const builtins: Preset[] = [
  {id:'builtin-raw', name:'原音', chain:[]},
  {id:'builtin-metal', name:'金属音', chain:[node('frequency',{shift_hz:100},'metal-f'),node('distortion',{drive_db:6,mix:25},'metal-d'),node('gain',{gain_db:-6},'metal-g')]},
  {id:'builtin-phone', name:'电话音', chain:[node('highpass',{cutoff:300},'phone-h'),node('lowpass',{cutoff:3400},'phone-l')]},
  {id:'builtin-female', name:'女声', chain:[
    node('gate',{threshold_db:-50},'female-gate'), node('highpass',{cutoff:80},'female-high'),
    node('pitch',{semitones:5},'female-pitch'), node('formant',{semitones:2.5},'female-formant'),
    node('eq',{low_gain_db:-2,mid_gain_db:1,mid_freq:2400,high_gain_db:2},'female-eq'),
    node('compressor',{threshold_db:-20,ratio:2,makeup_db:2},'female-compressor'), node('gain',{gain_db:-3},'female-gain')]},
  {id:'builtin-blown-mic', name:'炸麦', chain:[
    node('highpass',{cutoff:120},'blown-high'),
    node('compressor',{threshold_db:-24,ratio:6,attack_ms:2,release_ms:80,makeup_db:6},'blown-compressor'),
    node('gain',{gain_db:6},'blown-drive'), node('distortion',{drive_db:24,mix:100,clip:85},'blown-distortion'),
    node('lowpass',{cutoff:6500},'blown-low'), node('gain',{gain_db:-12},'blown-output')]},
  {id:'builtin-broadcast', name:'广播声', chain:[
    node('gate',{threshold_db:-48},'broadcast-gate'), node('highpass',{cutoff:80},'broadcast-high'),
    node('eq',{low_gain_db:2,low_freq:150,mid_gain_db:1,mid_freq:1500,high_gain_db:1},'broadcast-eq'),
    node('compressor',{makeup_db:3},'broadcast-compressor'), node('gain',{gain_db:-3},'broadcast-output')]},
  {id:'builtin-ethereal', name:'空灵声', chain:[
    node('gate',{threshold_db:-50},'ethereal-gate'), node('pitch',{semitones:3},'ethereal-pitch'),
    node('formant',{semitones:1},'ethereal-formant'), node('eq',{high_gain_db:2},'ethereal-eq'),
    node('reverb',{room:70,damping:45,mix:25},'ethereal-reverb'), node('gain',{gain_db:-4},'ethereal-output')]}
];
export const emptySession: Session = {schemaVersion:1,inputId:'',outputId:'',monitorId:'',monitoring:false,closeToTray:false,bufferMs:5,chain:[],presets:[]};
function validateChain(value: unknown): FilterInstance[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error('滤镜数量应在 0–64 之间');
  const ids = new Set<string>();
  if (value.filter(v => v?.type === 'rvc').length > 1) throw new Error('每条管线最多支持一张 AI 卡片');
  return value.map(v => {
    if (!v || typeof v !== 'object' || typeof v.id !== 'string' || !v.id || ids.has(v.id) || typeof v.enabled !== 'boolean') throw new Error('滤镜实例格式无效');
    ids.add(v.id);
    const definition = definitions.find(d => d.type === v.type); if (!definition) throw new Error('未知滤镜类型');
    const params: Record<string,number> = {};
    for (const p of definition.parameters) {
      const legacyClip = definition.type === 'distortion' && p.key === 'clip' && v.params && typeof v.params === 'object' && !Object.hasOwn(v.params, 'clip');
      const n = legacyClip ? 0 : v.params?.[p.key]; if (typeof n !== 'number' || !Number.isFinite(n) || n < p.min || n > p.max) throw new Error('滤镜参数超出范围');
      params[p.key] = n;
    }
    let model: AiModel | undefined;
    if (v.model !== undefined && v.model !== null) {
      if (definition.type !== 'rvc' || typeof v.model.path !== 'string' || !v.model.path.toLowerCase().endsWith('.pth')
        || typeof v.model.index !== 'string' || (v.model.index && !v.model.index.toLowerCase().endsWith('.index'))
        || [v.model.path,v.model.index].some(p => p.length > 32767 || p.includes('\0'))) throw new Error('AI 模型路径无效');
      model = {path:v.model.path,index:v.model.index};
    }
    return {id:v.id,type:definition.type,enabled:v.enabled,params,...(model ? {model} : {})};
  });
}
export function validateSession(value: unknown): Session {
  const v = value as Session;
  if (!v || v.schemaVersion !== 1 || !['inputId','outputId','monitorId'].every(k => typeof v[k as keyof Session] === 'string') || typeof v.monitoring !== 'boolean' || !Number.isFinite(v.bufferMs) || v.bufferMs < 5 || v.bufferMs > 30 || !Array.isArray(v.presets) || v.presets.length > 100) throw new Error('保存的配置格式无效');
  const presetIds = new Set<string>();
  const presets = v.presets.map(p => { if (!p || typeof p.id !== 'string' || !p.id || presetIds.has(p.id) || typeof p.name !== 'string' || !p.name.trim() || p.name.length > 80) throw new Error('预设格式无效'); presetIds.add(p.id); return {...p, chain: validateChain(p.chain)}; });
  if (v.closeToTray !== undefined && typeof v.closeToTray !== 'boolean') throw new Error('关闭窗口设置无效');
  return {...v, closeToTray:v.closeToTray ?? false, chain:validateChain(v.chain),presets};
}
