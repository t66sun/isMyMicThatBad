# isMyMicThatBad

[English](../README.md) | 中文

一款 Windows 变声器，带磨砂玻璃滤镜卡片编辑器、可配置音频链、预设和可选的关闭到托盘行为。

从 [GitHub Releases](https://github.com/t66sun/isMyMicThatBad/releases) 下载 Windows x64 便携 ZIP。使用说明和已知音频限制见[用户指南](user-guide_zh.md)。

完整 AI 下载包按 GitHub 附件大小限制拆成编号分卷。将全部 ZIP 分卷和 `Join-Release.ps1` 下载到同一目录，运行脚本校验并合并后，再解压得到的 ZIP。包内附带本地 NVIDIA AI 环境和默认音色模型，不包含个人设置。

## 架构

麦克风进入有界 WASAPI 队列、有序单声道 DSP 链和 CABLE Input；其他应用捕获 CABLE Output。可选耳机监听独立调整时钟。处理使用 48 kHz、256 帧块。

- React 拥有会话和滤镜卡片编辑器。拖动预览和选择是临时的；有效放置通过校验后的原生命令提交链路。
- `src/app/src/filters.json` 是共享目录和参数契约。Rust 在 `dsp.rs` 和 `effects.rs` 中构造处理器前校验它。
- `audio.rs` 拥有流、预分配队列和 worker；`clock.rs` 处理重采样。`graph.rs` 保留处理器状态并交叉淡化实时更改。音频回调使用预分配处理缓冲区。
- `settings.rs` 持久化配置并保留损坏的原始文件。`main.rs` 拥有窗口／托盘行为和共享音频控制器。加载设置从不启动采集。
- `ai.rs` 拥有隔离的本地 CUDA worker 和有界音频队列。一条 RVC v1/v2 卡片可位于链中任意位置；模型和可选索引随预设保存。AI 失败停止输出。准备好的 Python 运行时和基础模型支持离线使用。

编译扩展点见[滤镜开发](filter-development_zh.md)，延期延迟工作见[音频优化](audio-optimization_zh.md)。在 GitHub Issues 和 Releases 中跟踪实现任务和发布历史。

## 源码与构建

`src/app/src` 包含 React 编辑器；`src/native` 包含我们的 Rust 音频引擎、窗口／托盘行为和原生构建配置。`scripts/Initialize-Native.ps1` 将此源码复制到被忽略的 `src/app/src-tauri` 工作区。编辑 `src/native`，不要编辑生成的副本。研究检出、依赖源码、供应商驱动／运行时文件和构建输出均从 Git 排除。

在 Windows 上安装 Node.js、Rust MSVC 工具链、Visual Studio C++ Build Tools、Windows SDK 和 libclang（如需要设置 `LIBCLANG_PATH`）。运行：

```powershell
./scripts/Build.ps1 -ChecksOnly
./scripts/Build.ps1
```

本地工作区还必须包含 `src/app/src-tauri/resources/vb-cable` 下的原始 VB-CABLE 再分发文件，以及 `resources/msvc-runtime` 下的 Microsoft 可再分发 DLL，用于便携包。遵守供应商的再分发条款；这些第三方文件不是本仓库的源码资产。优化构建创建无应用安装程序的可执行文件。`scripts/Package.ps1` 在创建便携 ZIP 前要求干净的已提交源码树和匹配的本地验收记录。

要包含准备好的 AI 运行时和个人设置，运行 `./scripts/Package.ps1 -AiRuntimePath <已准备运行时目录> -SettingsPath <settings.json>`。包将 Python、RVC、基础模型、默认 Saisho Utane 模型、包版本及其许可声明复制到 `data/ai`；下载缓存和测试录音被排除。没有 `-AiRuntimePath` 时，应用在首次使用时准备运行时。解压整个 ZIP 后运行；设置在音频停止时加载。包含的 CUDA 环境针对已测试的 Windows x64 NVIDIA 机器，VB-CABLE 和 WebView2 仍是系统前提。

`tests/tray.mjs` 中的原生托盘回归使用隔离的便携副本、本地 Edge WebDriver 和 PowerShell 7 进行 Windows UI Automation。它验证处理状态、持久化、恢复／退出和启动失败反馈，不保存麦克风音频。
