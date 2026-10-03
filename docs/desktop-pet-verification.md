# 桌面 Pet 验证说明

## 生命周期

主窗口拥有 Pet 窗口。程序启动时创建 Pet；当计时器处于
`NotStarted` 状态时，Pet 保持显示。

开始计时后，Pet 通过短暂的滑出动画隐藏。计时结束时，主窗口停止计时器；
如果 Pet 有一部分位于屏幕外，则先将它移动到当前显示器的工作区内，并使其
完全可见，然后显示 Pet，播放一次性的 `jump` 动画。动画结束后 Pet 保持
`idle` 状态，直到用户点击确认提醒。点击 Pet 后，Pet 滑出隐藏并开始下一轮
计时。Pet 可见且没有进行交互时，会在短暂的 idle 延迟后原地播放 `walk`
动画；当前版本不会因为播放 Walk 而改变 Pet 的窗口位置。

关闭主窗口时，程序会先停止计时器并销毁 Pet 窗口；随后释放 Pet 的渲染器
和像素表面资源。

## 资源契约

项目中的角色资源由 `assets/pets/cat-dog/manifest.json` 描述。加载器会为
manifest 中的每个角色建立目录；当前每个角色都要求存在 `idle`、`walk` 和
`jump` 动画。每个动画至少需要一帧，帧文件按照 manifest 中的文件名模式，
以数字顺序解析。

角色通过配置文件中的 `character` 字段选择，默认值为 `cat`。例如将其设置
为 `dog` 后，下一次启动程序会使用 Dog 资源；未知角色会回退到 `cat`。

气泡文本通过配置文件中的 `speech_bubble` 字段配置。每条消息包含 `text`
和 `display_duration_ms`，消息之间的隐藏间隔由 `hidden_gap_ms` 配置。例如：

```json
"speech_bubble": {
  "messages": [
    { "text": "Time to stretch!", "display_duration_ms": 5000 },
    { "text": "站起来活动一下吧！", "display_duration_ms": 4000 }
  ],
  "hidden_gap_ms": 10000
}
```

PNG 的宽高按原始像素尺寸处理。Pet 窗口和分层窗口使用的 BGRA 表面直接使用
这些尺寸；程序不会再次对已经解码的 PNG 像素应用 DPI 缩放。

## Windows 冒烟测试

在仓库根目录执行：

```powershell
cargo fmt -- --check
cargo test
cargo build --release
```

然后在 Windows 上手动确认：

1. 在计时器尚未开始时启动程序，确认 Pet 可见。
2. 等待片刻，确认 Pet 从 `idle` 切换到 `walk` 并原地踏步，窗口位置保持不变。
3. 开始计时并暂停或重置，确认开始计时会隐藏 Pet，重置后 Pet 保持隐藏，
   直到下一次提醒流程显示它。
4. 在多个显示器之间拖动 Pet，包括 DPI 缩放比例不同的显示器。确认 Pet
   可以部分超出屏幕边缘，同时仍有一小部分保持可见。
5. 等待计时结束。确认如果 Pet 之前有部分位于屏幕外，它会先以最小距离
   移动到完全可见的位置，然后播放 `jump` 并回到 `idle` 状态。
6. 点击 Pet，确认它滑出隐藏并开始下一轮计时。
7. 检查 Pet 和托盘图标的右键菜单，确认包含“开始计时”、“显示主窗口”和
   “退出”。
8. 启用“关闭时缩小到系统托盘图标”，关闭主窗口，确认托盘图标仍然存在，
   并且可以重新打开或退出程序。
9. 在配置文件中将 `character` 设置为 `dog`，重新启动程序，确认 Pet 使用
   Dog 的 `idle`、`walk` 和 `jump` 动画。
10. 配置两条气泡消息，确认消息按顺序显示，各自持续配置的时长，并在消息
    之间保持配置的隐藏间隔；开始计时、暂停和重置时确认气泡也会隐藏。

## 延后工作

位置保存、基于 alpha 的命中测试、更多设置，以及 Direct2D/DirectComposition
仍不属于 MVP 范围。Pet 的 `walk` 永远是原地播放，不会自动改变窗口位置。
