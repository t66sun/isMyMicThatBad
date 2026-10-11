import type { AiStatus } from './api';
import { native } from './api';
import type { AiModel, FilterInstance } from './model';

export function AiControls({ node, status, locked, running, onPrepare, onChoose, onModel }: {
  node: FilterInstance; status: AiStatus; locked: boolean; running: boolean;
  onPrepare: () => void; onChoose: (index: boolean) => void; onModel: (model?: AiModel) => void;
}) {
  const modelPath = node.model?.path ?? status.modelPath;
  const indexPath = node.model?.index ?? status.indexPath;
  const disabled = locked || running || status.preparing;
  return <div className="ai-controls" aria-label="AI 音色与环境" aria-busy={status.preparing}>
    <div className="ai-environment">
      <strong>本地 AI 环境</strong>
      <span role="status">{status.preparing ? '正在下载并准备，请保持网络连接…' : status.ready ? '已就绪 · 音频在本机处理' : '尚未准备 · 首次下载数 GB，之后可离线使用'}</span>
      <button className="secondary-button" disabled={!native || disabled || status.ready} onClick={onPrepare}>{status.preparing ? '正在准备…' : status.ready ? '环境已就绪' : '准备 AI 环境'}</button>
      {status.error ? <p role="alert" className="ai-error">{status.error}</p> : null}
    </div>
    <div className="ai-model">
      <strong>目标音色</strong>
      <span className="ai-path" title={modelPath}>{node.model ? modelPath : '默认测试音色 · Saisho Utane'}</span>
      <div className="ai-buttons">
        <button className="secondary-button" disabled={!native || disabled} onClick={() => onChoose(false)}>导入 .pth 模型</button>
        <button className="secondary-button" disabled={disabled || !node.model} onClick={() => onModel()}>恢复默认音色</button>
      </div>
      <span className="ai-path" title={indexPath}>索引：{indexPath || '未使用（可选）'}</span>
      <div className="ai-buttons">
        <button className="secondary-button" disabled={!native || disabled || !modelPath} onClick={() => onChoose(true)}>导入 .index 索引</button>
        <button className="secondary-button" disabled={disabled || !modelPath || !indexPath} onClick={() => onModel({path:modelPath,index:''})}>清除索引</button>
      </div>
      {running ? <p className="hint">更换模型或准备环境前，请先停止处理。</p> : null}
      {!native ? <p className="hint">浏览器可编辑 AI 参数；准备环境和导入文件请使用桌面应用。</p> : null}
      {!node.model ? <p className="hint">默认声音由 Saisho Utane 创建，<a href="https://huggingface.co/Saisho-Utane/Saisho-Model-RVC" target="_blank" rel="noreferrer">CC BY 4.0</a>。导入模型后请遵守其作者许可。</p> : null}
      <p className="hint">模型或推理失败时停止处理并提示错误，不会自动输出原音。</p>
    </div>
  </div>;
}
