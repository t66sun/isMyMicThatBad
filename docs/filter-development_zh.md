# 添加编译型滤镜

[English](filter-development.md) | 中文

引擎以 48 kHz 处理单声道 f32 块。请编辑 `src/native/src` 中的原始源码；Tauri 工作区由[构建脚本](README_zh.md#源码与构建)生成。

1. 复制 `template.rs` 并实现 `Filter::process`、`set_params` 以及 `latency()` 中的算法延迟。在构造函数中分配历史／暂存缓冲区；处理期间避免分配、阻塞锁、磁盘访问和日志。
2. 在 `src/app/src/filters.json` 中添加类型、显示名、描述、颜色和数值范围／默认值。Rust 校验与编辑器共享该目录。可选的 `summaryKeys` 选择最多两个卡片显示值；所有参数仍保留在检查器中。
3. 在 `dsp.rs` 的 `Chain::new` 中注册构造函数。编辑器根据元数据生成卡片和控件。
4. 通过 `tests/audio_pipeline.rs` 中的 `Chain` 测试：已知信号、跨块状态、独立实例以及平滑的参数／旁路过渡。

模板的 `amount_db` 控件使用 10 ms 平滑。要注册副本，将以下定义追加到 `filters.json`：

```json
{"type":"user-trim","name":"自定义滤镜","description":"可修改的滤镜模板。","color":"#75bfc7","parameters":[{"key":"amount_db","label":"强度","unit":"dB","min":-24,"max":12,"step":0.1,"default":0}]}
```

将 `"user-trim" => Box::new(crate::template::TemplateFilter::new(config.params["amount_db"]))` 添加到构造函数的 match 并重新构建。这是编译型扩展点；没有运行时脚本／DLL 加载器。

`effects.rs` 中的音高／共振峰处理使用 Signalsmith Stretch 包装器 0.1.3，窗口为 2048 帧、hop 为 64 帧。相邻音高／共振峰卡片共享分析，同时保留独立控件和旁路。零移位使用对齐的干声。声明完整延迟用于补偿。包装器的 formant-base setter 错误地转发到 formant shift；保留默认分析。

## 硬件检查

停止其他音频发送者并使用隔离的便携副本。`isMyMicThatBad.exe --devices` 列出端点；`--smoke 5` 启动物理麦克风和 VB-CABLE 并执行一次实时滤镜编辑。`--smoke 1800` 是可选的 30 分钟稳定性运行。

初始化本地 Tauri 工作区后，运行可选的延迟测试：

```powershell
cargo test --release --features custom-protocol --manifest-path src/app/src-tauri/Cargo.toml --test hardware_latency microphone_to_cable_p95_is_under_150ms -- --ignored --nocapture
```

使用原生 f32 / 48 kHz 端点，关闭监听并使用 5 ms 缓冲。从 `--devices` 设置 `MIC_TEST_INPUT_ID` 以便重复比较。校准测量原始和零频移／增益链路；静音或不相关输入无法得出结论，聊天应用缓冲不在其中。

对于四个新预设，使用 `node src/app/node_modules/tsx/dist/cli.mjs tests/export-presets.ts <absolute-json-path>` 导出共享定义，将 `MIC_TEST_PRESETS_FILE` 设为该路径，然后运行 `new_presets_microphone_to_cable_p95_is_under_200ms`。它使用同预设 DSP 参考和源时间戳，保留算法与硬件延迟，要求至少 20 次匹配和 80% 覆盖率，并拒绝应用欠载／溢出。它不证明长期稳定性。音频保留在内存中。

使用相同的麦克风和预设环境变量，运行 `female_preset_runs_without_underruns` 进行 30 分钟女声预设回归。它打开 CABLE Output 读取器，要求连续交付且应用队列欠载／溢出为零，并单独记录可恢复的设备通知。样本在内存中排空。`MIC_TEST_SECONDS` 可将诊断运行缩短到 10–1800 秒，默认 1800。预设延迟测试也拒绝任何欠载。运行可选的 `audio::tests::output_reader_quiet_tail_has_realtime_headroom` 库测试（优化构建）以分析正常音频和极小尾音下的输出 sinc 回调。
