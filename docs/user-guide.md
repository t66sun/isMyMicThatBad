# isMyMicThatBad 使用说明

## 开始使用

1. 将便携 ZIP 解压到可写目录，运行 isMyMicThatBad.exe。此版只提供便携包。
2. 界面依赖系统 WebView2。缺少时从 https://developer.microsoft.com/microsoft-edge/webview2/ 安装 Evergreen Runtime。
3. 未安装 VB-CABLE 时，点击应用内“安装随包附带的 VB-CABLE”，在官方安装窗口操作并允许管理员提示，然后重启 Windows。
4. 输入选择真实麦克风，输出选择 CABLE Input；聊天软件的麦克风选择 CABLE Output。
5. 点击“开始处理”。耳机监听默认关闭，开启前选择不同于主输出的耳机设备。

portable 只表示本应用无需安装；VB-CABLE 仍需作为系统驱动安装。两个包均保留官方 VB-CABLE 文件。VB-CABLE 是 VB-Audio 的 Donationware，欢迎捐赠；来源：https://vb-audio.com/Cable/ 。分发说明：https://vb-audio.com/Services/licensing.htm 。

## 卡片管线

从左侧拖入滤镜或点击添加。也可点击管线中某个位置的加号，再点击左侧滤镜插入该位置。卡片从上到下处理；拖动手柄排序，也可使用上下按钮或在手柄上按 Alt+上下方向键。支持复制、删除、旁路和调参，同一滤镜可重复使用。

频率平移按固定 Hz 加减每个频率，会产生金属或不自然音色；它不是按半音升降调。内置预设为原音、金属音、电话音。保存当前管线后可命名加载；重启恢复设置但不会自动开始采集。

安装版配置位于 %APPDATA%/com.t66sun.ismymicthatbad/settings.json，portable 位于旁边的 data/settings.json。配置损坏时保留 settings.corrupt-*.json，目录不可写时显示保存错误，不静默丢失旧配置。

## 设备与缓冲

运行时更换设备需要先停止。监听设备出错时关闭监听，主输出保持运行；主输入或输出出错时停止处理并保留卡片配置。若发生断音，提高“缓冲目标”后重新开始，并检查 Windows 音频格式及 VB-CABLE 控制面板。

内部处理为 48 kHz 单声道，按设备实际格式重采样。频移滤镜有算法延迟；运行中删除滤镜后保留已有延迟补偿，停止/重启会重新计算。首版验收目标调整为指定测试链路 p95 ≤150 ms；64 张卡片是配置上限，不承诺任意长链都达到这一目标。

新配置的缓冲目标默认 5 ms，已有配置保持原值。Intel 麦克风、VB-CABLE 当前 Latency=3072 samples / Internal SR=48000 Hz、监听关闭、5 ms 缓冲下，空链 p95=137.81 ms，频移零平移加 -6 dB 增益链 p95=140.26 ms，达到本阶段 150 ms 目标。其他设备、长链和聊天软件额外缓冲需另行测量，实测状态见 [音频验收记录](../research/audio-acceptance/README.md)。界面不将缓冲目标冒充端到端测量结果。欠载计数记录程序输出队列断供的事件，溢出记录丢弃的样本，设备中断记录驱动报告的短暂断流；这些通知不会被误当成设备断开。

本版按用户要求跳过完整 30 分钟稳定性测试；已完成的约 9 分钟测试未出现新增欠载、溢出或驱动中断。长期稳定性仍需后续验收。
