# 实时变声器开源参考

调研日期：2026-10-08。目标是麦克风输入 → 可叠加的实时 DSP 滤镜 → 虚拟麦克风输出，并提供预设。当前开发环境为 Windows，因此优先检查 Windows 路由，再补充 Linux 效果链与官方驱动参考。

## 结论

找到了相近项目，但没有核实到一个现成项目同时满足全部要求。TIMBRE 最接近“变声预设 + 有序滤镜节点 + Windows 输出”；VoxShift 提供较丰富的传统 DSP；MicLayer 提供原生 Rust 实时音频链。三者的 Windows 虚拟麦克风均依赖外部 VB-CABLE，不能把“向虚拟线缆写入声音”当作“项目实现了自己的虚拟麦克风驱动”。[TIMBRE README](https://github.com/GoncaloCalvo/timbre#what-it-is)、[VoxShift README](https://github.com/why-xdd/voxshift#-using-it-as-a-microphone-in-discord--games)、[MicLayer 虚拟麦克风说明](https://github.com/badoomtch/MicLayer/blob/main/docs/virtual-microphone.md)。

EasyEffects 最贴近“用户添加效果并调整顺序”，覆盖升降调、高低通、EQ、降噪等，但基于 Linux/PipeWire。SysVAD 是 Windows 驱动开发样例，其 capture/loopback 用生成的音调模拟，不是现成音频线缆。[EasyEffects](https://github.com/wwmm/easyeffects#effects-available)、[SysVAD README](https://github.com/microsoft/Windows-driver-samples/blob/main/audio/sysvad/README.md)。

## 六个参考仓库

| 项目 / 本地目录（相对本文件） | 平台与实现 | 已核实功能及与需求的差距 | 许可证 |
| --- | --- | --- | --- |
| [TIMBRE](https://github.com/GoncaloCalvo/timbre) / `repos/timbre/` | Windows 11；Electron、React、TypeScript、Web Audio、Tone.js | 麦克风捕获、监听和 VB-CABLE 双输出；8 个预设；升降调、环形调制、混响、失真、由高通和低通组成的带通。有序节点定义支持组合；现有 UI 调整预设内参数，未发现任意增删/重排效果的编辑器；注册表中未实现 EQ 或降噪。 | [MIT](https://github.com/GoncaloCalvo/timbre/blob/main/LICENSE) |
| [VoxShift](https://github.com/why-xdd/voxshift) / `repos/voxshift/` | Windows；Python、NumPy/SciPy、sounddevice、Tkinter | 实时变调和共振峰调整、3 段 EQ、失真、bitcrusher、环形调制、tremolo、vibrato、echo、reverb、noise gate；预设与参数调节；VB-CABLE 输出。源码效果顺序固定；noise gate 不能等同于持续背景噪声抑制。README 的约 35 ms 延迟未实测。 | [MIT](https://github.com/why-xdd/voxshift/blob/main/LICENSE) |
| [MicLayer](https://github.com/badoomtch/MicLayer) / `repos/MicLayer/` | Windows；Tauri、React、Rust | pre-alpha；输入增益、高通、RNNoise 降噪、gate、EQ、压缩、de-esser、limiter、输出增益；配置档与 VB-CABLE 后端。模块顺序固定；没有变调/失真；品牌虚拟驱动为后续计划。 | [MIT](https://github.com/badoomtch/MicLayer/blob/main/LICENSE) |
| [EasyEffects](https://github.com/wwmm/easyeffects) / `repos/easyeffects/` | Linux/PipeWire；C++、Qt/QML/Kirigami | 用户可调整效果顺序；pitch shift、低通/高通/带通/带阻、EQ、crusher、RNNoise、DeepFilterNet、gate 等；预设管理；使用 PipeWire 虚拟设备。可参考产品交互和链管理，但不是 Windows 程序。 | [GPL-3.0](https://github.com/wwmm/easyeffects/blob/master/LICENSE) |
| [noise-suppression-for-voice](https://github.com/werman/noise-suppression-for-voice) / `repos/noise-suppression-for-voice/` | C++；Windows VST 宿主等，另有 Linux/macOS 插件格式 | RNNoise 实时语音降噪插件；VAD 阈值、尾部/回溯宽限参数；README 明确要求 48 kHz，回溯宽限引入延迟。它是单个效果插件，不是完整变声软件或 Windows 虚拟设备驱动。 | [GPL-3.0](https://github.com/werman/noise-suppression-for-voice/blob/master/LICENSE) |
| [Windows-driver-samples / SysVAD](https://github.com/microsoft/Windows-driver-samples) / `repos/Windows-driver-samples/audio/sysvad/` | Windows；C++、WDM、WaveRT、WDK | 官方虚拟音频设备、端点与 APO 示例；用于参考如何暴露设备。没有预设或用户 DSP 链，没有把应用处理后的 PCM 注入 capture 的现成桥接；示例模拟采集和 loopback，仍需设计数据通路与产品化驱动签名。这里只检出 `audio/sysvad/` 与根文件。 | [MS-PL](https://github.com/microsoft/Windows-driver-samples/blob/main/LICENSE) |

表中功能依据各仓库 README 和以下源码核实；未编译、运行或测量实际音质与延迟。VB-CABLE 是独立第三方产品，不包含在上述应用自身的开源许可证内。[TIMBRE 要求](https://github.com/GoncaloCalvo/timbre#requirements)、[MicLayer 后端说明](https://github.com/badoomtch/MicLayer/blob/main/docs/virtual-microphone.md)。

## 源码阅读入口

- TIMBRE：[`src/renderer/src/audio/AudioEngine.ts`](https://github.com/GoncaloCalvo/timbre/blob/main/src/renderer/src/audio/AudioEngine.ts) 查看采集、节点连接和输出；[`effects.ts`](https://github.com/GoncaloCalvo/timbre/blob/main/src/renderer/src/audio/effects.ts) 查看效果注册表及有序预设；[`App.tsx`](https://github.com/GoncaloCalvo/timbre/blob/main/src/renderer/src/App.tsx) 查看预设选择与参数 UI。
- VoxShift：[`voxshift.py`](https://github.com/why-xdd/voxshift/blob/main/voxshift.py) 的 `Engine` 和 `self.chain` 查看固定效果顺序与设备输出；[`dsp.py`](https://github.com/why-xdd/voxshift/blob/main/dsp.py) 查看各 DSP 算法；[`driver.py`](https://github.com/why-xdd/voxshift/blob/main/driver.py) 查看外部线缆安装辅助逻辑。
- MicLayer：[`engine/audio/src/engine.rs`](https://github.com/badoomtch/MicLayer/blob/main/engine/audio/src/engine.rs) 查看音频运行；[`engine/dsp/src/graph.rs`](https://github.com/badoomtch/MicLayer/blob/main/engine/dsp/src/graph.rs) 查看固定处理序列；[`noise_suppression.rs`](https://github.com/badoomtch/MicLayer/blob/main/engine/dsp/src/noise_suppression.rs) 查看 nnnoiseless RNNoise 帧处理；[`engine/virtual-mic/src/vb_cable.rs`](https://github.com/badoomtch/MicLayer/blob/main/engine/virtual-mic/src/vb_cable.rs) 查看当前输出后端；`profiles/` 查看预设配置。
- EasyEffects：[`src/effects_base.cpp`](https://github.com/wwmm/easyeffects/blob/master/src/effects_base.cpp)、[`stream_input_effects.cpp`](https://github.com/wwmm/easyeffects/blob/master/src/stream_input_effects.cpp) 查看效果连接和输入流；[`presets_manager.cpp`](https://github.com/wwmm/easyeffects/blob/master/src/presets_manager.cpp) 查看预设；`src/pitch.cpp`、`filter.cpp`、`rnnoise.cpp` 查看算法集成。
- noise-suppression-for-voice：[`src/common/src/RnNoiseCommonPlugin.cpp`](https://github.com/werman/noise-suppression-for-voice/blob/master/src/common/src/RnNoiseCommonPlugin.cpp) 查看降噪缓冲与 VAD；[`src/juce_plugin/RnNoiseAudioProcessor.cpp`](https://github.com/werman/noise-suppression-for-voice/blob/master/src/juce_plugin/RnNoiseAudioProcessor.cpp) 查看音频插件处理。
- SysVAD：[`audio/sysvad/README.md`](https://github.com/microsoft/Windows-driver-samples/blob/main/audio/sysvad/README.md)、`sysvad.sln`、`EndpointsCommon/`、`TabletAudioSample/` 查看驱动构建、WaveRT 端点与 INF；不将示例生成的正弦音误认为应用音频路由。

## 后续参考方向

基于上述实现的判断：先读 TIMBRE 的有序效果数据模型、VoxShift 的变调/失真算法、MicLayer 的实时处理与线缆输出；参考 EasyEffects 的效果顺序和预设交互。如果要输出一个真正名为本软件的 Windows 麦克风设备，再单独研究 SysVAD 的端点、PCM 桥接和签名。这是实现路径判断，不是已完成的功能。

本次仅收集源码及说明；没有安装驱动、依赖或模型，没有执行下载的项目。六个仓库已浅克隆（depth=1），未初始化额外 submodule；微软仓库稀疏检出 audio/sysvad 与根文件。克隆来源、分支、确切提交和检出范围见 [repositories.json](repositories.json)。repos/ 是第三方研究检出，已通过本目录 .gitignore 排除，避免被主仓库当作嵌套 Git 仓库收录；这不影响本地源码阅读。
