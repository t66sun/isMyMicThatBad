# 音频延迟后续

[English](audio-optimization.md) | 中文

合格目标：原始／零频移增益校准 p95 ≤150 ms，四个新预设 ≤200 ms。早先的 120 ms 和 80 ms 目标仍然延期。当前测量和限制见[用户指南](user-guide_zh.md#设备与缓冲)。对于 v0.4.0，用户明确接受超出这些延迟目标以换取稳定音频；当前测量不是延迟目标通过。

## 需要保留的约束

- 合格配置：Intel 麦克风、VB-CABLE 3072 samples / 48 kHz、5 ms 应用缓冲、关闭监听。仅线缆 p95 测得 56.29 ms；该边界与完整路径不同，因此不能通过相减隔离应用延迟。
- 固定 10 ms WASAPI 输出缓冲请求未降低完整路径延迟，已撤回。将输出预留从两个设备周期降到一个是导致 19 次调试运行和 16 次优化运行欠载，已撤回。当前源码保留三个标称输出周期（受队列容量限制），启动时额外保留一个周期；实际 WASAPI 回调可能超过标称周期。在降低该预留前重新鉴定延迟。
- 已启用 CPAL 实时调度和 DSP worker 优先级。回调唤醒 worker。离线 DSP 余量不能证明回调截止时间或端到端延迟。
- 变换后的声音需要同预设处理参考，用源时间戳减去声明的 DSP 延迟。直接比较原始波形不可靠；安静或低相关运行不是通过证据。

## 当前稳定性回归

输出队列现在保存完整 DSP 块，监听预热为 sinc 输入历史留出空间。输出重采样前将低于 1e-20 的不可听样本替换为零：优化后的三秒极小尾音探测从约 0.372 s 降到 0.007 s，不改变可听样本或样本时序。共享预设 DSP 余量测试也覆盖安静尾音。

在上述合格麦克风／线缆配置下，三次连续的优化 10 秒女声预设启动均通过，持续向 CABLE Output 读取器交付，应用欠载／溢出为零。随后运行在最后一条 1230 秒状态记录后按用户要求停止，应用队列计数同样为零；这不是完整的 30 分钟通过。可恢复的驱动通知仍可观察到。完整路径 p95 未重新测量，额外预留可能增加延迟。

## 延期工作

| 优先级 | 调查 | 保留变更前所需的证据 |
| --- | --- | --- |
| 1 | 在女声预设预留和安静尾音修复后重新鉴定延迟和完整时长稳定性；将驱动通知与应用饿死分开调查。 | 匹配的硬件／配置前后 p95 和计数；启动后 30 分钟稳定性且计数不增长。 |
| 2 | 根据实际缓冲统计调整 VB-CABLE，保留 48 kHz 和回滚设置。 | 完整路径相关性通过，且没有额外驱动故障。 |
| 3 | 只有在解释清楚更小预留为何饿死后才减少应用缓冲。 | 监听、实时编辑和 CPU 负载下稳定的 FIFO 填充和回调截止时间；不为达标丢弃样本。 |
| 4 | 只有后端被证明是测量瓶颈时，才评估 Windows 低延迟共享模式周期。 | 已证明的主机缓冲贡献，以及与麦克风、线缆和监听设备的兼容性。 |
| 5 | 在更改重采样质量或滤波器长度前分析 sinc 转换和长链。 | CPU 节省、频率响应、独立时钟稳定性和可听比较。 |

完整稳定性和人工音色自然度评估仍然延期。硬件结果不包含聊天应用缓冲、其他设备和任意 64 卡链路。保留本地原始测量和已完成的发布包作为证据。

v0.4.0 发布在采集前以静音开始渲染，保留三周期 FIFO 和额外启动预留。读取器等待预填，输入时钟校正只采样完整输入块。最终物理校准、60 秒女声预设播放和 10 秒静音检查通过，应用欠载／溢出为零。完整 30 分钟和主观检查仍然延期。

## 固定链 RVC 故障取证

使用优化构建和保存的六卡配置。在项目工具链终端中执行 `cargo test --release --features custom-protocol --manifest-path src/app/src-tauri/Cargo.toml --test hardware_latency --no-run` 构建观察器，将命令输出的测试程序路径传给脚本。每批试验先建立新的带日期证据目录。

```powershell
./tests/Run-ChainEvidence.ps1 -TestExe $testExe -SessionPath $session -AiRoot $preparedAi -EvidenceDirectory $evidence -StimulusWav $speech -SpeakerId $physicalSpeaker -CableReaderId $cableOutput -Scenario six-on -Repeats 5 -Seconds 90
```

分别以 `six-off`、`rvc` 和 `dsp` 运行匹配对照，以 `digital` 运行扬声器回环。测试语音会从扬声器播放。脚本在生成的构建目标目录中准备独立运行时，每轮单独保存配置、首错状态、可用的 GPU 日志、逐块计时、限幅前峰值和各端点能量。麦克风音频只在内存中处理。比较延迟时保持设备路径、电平、缓冲和监听状态一致。`tests/summarize_chain_evidence.py` 可以汇总原始结果。应用在故障停止后保留队列计数，直到再次启动或主动停止。上一轮 worker 日志位于 `ai/worker.previous.log`。

静音测试使用同一个观察器，构建时启用 `--features custom-protocol,acceptance-replay`。在上述命令加上 `-SilentReplay`，省略 `-SpeakerId`。该模式在真实 48 kHz 麦克风回调内用 WAV 代替采样，分别记录采集时钟与 CABLE Output。耳机监听正常处理，最终向物理端点写零，时间线保留写零前的电平。测量房间声学链或耳机端点本身时使用有声模式。

离线音质对照使用 `tests/chain_quality.py prepare` 和本地 48 kHz 单声道 PCM16 语音生成素材。将新素材及命令输出的时长传入 `tests/ai_inference.py`，指定 `--output-directory` 和 `--float-output`。把六卡配置和导出的四个预设作为 `session.json`、`presets.json` 复制到音质目录。将 `MIC_TEST_QUALITY_DIR` 指向该目录，运行优化版 audio_pipeline 中显式忽略的 `quality_probe::render_matching_inputs_at_each_card`，再执行 `tests/chain_quality.py summarize`。汇总记录造成波形变化的级别及随机试听片段的实测响度，人工听评与客观指标分开记录。
