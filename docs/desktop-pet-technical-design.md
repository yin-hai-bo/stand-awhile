# 桌面宠物技术方案

## 目标与范围

本文档描述一个独立的 Windows 桌面宠物功能：在桌面或普通应用窗口之上显示一个小型、带透明背景的动画角色，支持动画播放、鼠标交互、窗口移动、置顶、穿透和多显示器环境。

目标体验类似 Microsoft Agent，也参考 VPet 的 Pet 显示方式，但不重写 VPet，也不要求复用它的代码。实现语言可以是 Rust 或 C++，GUI 必须直接使用 Win32 API；DirectX 仅负责渲染和合成相关工作。

非目标：桌面图标下方的壁纸级渲染、注入其他进程、修改用户应用窗口内容、复杂游戏逻辑和跨平台 GUI。

## 总体判断

最小可行实现不需要 Direct3D。一个 Win32 分层窗口（layered window）配合 `UpdateLayeredWindow`，就可以显示带 alpha 的 PNG 帧，并覆盖在桌面或普通应用窗口之上。

建议分两阶段：

1. 先用 Win32 分层窗口完成正确的窗口行为、动画时钟和输入。
2. 在明确遇到 CPU 合成、缩放、滤镜或大量粒子效果瓶颈后，再把内部渲染后端替换为 Direct3D 11/Direct2D 或 Direct3D 11 + Direct2D。

这样可以把“桌面窗口行为”和“图形渲染后端”放在两个独立模块中，避免一开始把所有问题绑定到 GPU。

## 运行时分层

```text
应用入口
  ├─ Win32WindowAdapter       创建窗口、层级、DPI、显示器、输入
  ├─ PetController            状态机、动作选择、移动和交互
  ├─ AnimationPlayer          时间轴、帧切换、循环、暂停、停止
  ├─ AssetPipeline             加载、解码、预乘 alpha、缓存
  └─ Renderer                  目标表面、合成、提交到窗口
       ├─ SoftwareLayeredRenderer  最小版本：CPU 位图 + UpdateLayeredWindow
       └─ D3D11Renderer             可选版本：GPU 纹理 + Direct2D/Direct3D
```

其中 `PetController` 不应该知道窗口句柄、D3D 设备或 PNG 文件；它只产生“当前姿态、位置、透明度和交互状态”。`Renderer` 不应该决定角色什么时候走、什么时候说话；它只消费一个可渲染快照。

这是本项目最重要的 seam：

```text
PetController -> RenderSnapshot -> Renderer
```

`RenderSnapshot` 可以包含：

- 世界/屏幕坐标；
- 当前动画帧或纹理句柄；
- 目标尺寸和缩放；
- alpha 和可见性；
- 是否允许命中鼠标；
- 当前帧的绘制偏移。

渲染器通过这个小 interface 隐藏窗口合成、GPU 资源和缓存细节，使软件渲染器和 DirectX 渲染器可以互换。

## 窗口实现

### 基本窗口样式

创建一个普通顶层 Win32 窗口：

- `WS_POPUP`：无边框；
- `WS_EX_TOOLWINDOW`：不进入 Alt+Tab；
- `WS_EX_LAYERED`：支持透明分层；
- 可选 `WS_EX_NOACTIVATE`：显示和鼠标移动时不抢前台焦点；
- 不要默认设置 `WS_EX_TRANSPARENT`，因为它会影响命中和输入；只在用户开启穿透时动态添加。

窗口初始大小应根据资源的逻辑画布计算，内部使用逻辑像素；真正放置到屏幕时再根据 DPI 转换成物理像素。

### 显示在应用窗口之上

“在 APP 窗口之上”通常只意味着桌宠窗口是 topmost，而不是把内容注入 APP。实现方式：

- 普通模式：`SetWindowPos(hwnd, HWND_TOP, ...)`；
- 置顶模式：`SetWindowPos(hwnd, HWND_TOPMOST, ...)`，并保持 `SWP_NOMOVE | SWP_NOSIZE`；
- 随时根据用户设置切换 `HWND_TOPMOST` 和 `HWND_NOTOPMOST`；
- 不要使用无限强制置顶循环，否则会干扰全屏应用、弹窗和系统安全界面。

如果目标是只覆盖某一个普通 APP，而不是所有窗口，可以监听前台窗口变化，并将宠物设置为该窗口的 owner/相对层级。但这条路径对全屏、UAC、管理员权限和多桌面行为更复杂，第一版不建议采用。

### 分层窗口的正确更新方式

软件后端使用：

1. 创建 DIB section，像素格式为 32 位 BGRA；
2. 使用预乘 alpha；
3. 将当前动画帧绘制到 DIB；
4. 调用 `UpdateLayeredWindow` 或 `UpdateLayeredWindowIndirect`；
5. 通过 `BLENDFUNCTION` 设置 `AC_SRC_ALPHA`。

不要把透明 PNG 直接作为普通窗口背景，也不要依赖颜色键透明；颜色键会丢失半透明边缘，头发、阴影和抗锯齿轮廓会出现黑边或白边。

### 鼠标命中与穿透

默认应让窗口接收鼠标消息，并在窗口过程里根据 alpha 或角色 hitbox 判断是否命中：

- 窗口外部透明区域返回 `HTTRANSPARENT` 或不触发动作；
- 角色可交互区域由动画元数据定义，而不是简单使用整张窗口；
- 全局穿透模式动态添加 `WS_EX_TRANSPARENT`；
- 关闭穿透时移除该扩展样式，并重新调用 `SetWindowPos` 使样式生效。

需要区分两种概念：

- 窗口穿透：事件交给后面的窗口；
- 角色透明区域不命中：窗口仍存在，但透明像素不触发角色动作。

## 动画资源管线

### 推荐资源模型

第一版建议支持两种格式：

1. 带 alpha 的 PNG 精灵图；
2. 帧序列目录，每张图片带帧时长元数据。

APNG 可以后续支持，但不应作为渲染器的核心抽象。渲染器最终只需要得到：

```text
AnimationClip {
    canvas_size
    frames: [Frame { image, duration_ms, pivot, hitboxes }]
    loop_mode
}
```

### 预处理

加载阶段完成：

- 解码图片；
- 统一为 premultiplied BGRA 或 GPU 可上传的 RGBA；
- 限制最大纹理尺寸；
- 生成 mipmap（如果需要缩小显示）；
- 计算每帧边界和可选 alpha hit-test 数据；
- 把资源放入内存缓存。

不要在每一帧从磁盘读取图片，也不要在 UI 线程解码图片。

精灵图不是必须的。对于 Direct3D 后端，更自然的方式是将各帧上传为纹理数组或纹理图集；对于软件后端，精灵图可以减少文件访问。

## 动画时钟

不要用递归调用和 `Sleep` 作为最终实现。推荐使用单调时钟：

- C++：`QueryPerformanceCounter`；
- Rust：`std::time::Instant`，或使用 Win32 的高精度计时器；
- 每次 tick 计算 `elapsed`；
- 根据累计时间选择帧；
- 渲染提交由消息循环或渲染线程驱动。

抽象上应使用：

```text
AnimationPlayer::update(now) -> FrameSelection
```

而不是让动画对象自己创建线程和直接操作窗口。

推荐状态：

```text
Stopped -> Playing -> Paused
              ├── loop back to Playing
              └── finish -> Stopped
```

动画更新频率和窗口更新频率可以不同。静态动画不需要持续提交窗口；只有帧或位置变化时才调用渲染器。

## DirectX 路线

### 推荐技术组合

在 Windows 上建议优先考虑：

```text
D3D11 device/context
    + DXGI swap chain 或 offscreen texture
    + Direct2D/DirectWrite（如需文字和 2D 图形）
    + DirectComposition 或 UpdateLayeredWindow（窗口提交）
```

Direct3D 11 的生态、调试工具和 Rust 绑定相对成熟。D3D12 对一个小桌面宠物通常过度复杂，除非后续需要大量粒子、复杂后处理或与已有 D3D12 引擎共享设备。

### 两种透明合成路线

#### 路线 A：D3D11 离屏渲染 + 读回 CPU + UpdateLayeredWindow

流程：

```text
GPU 纹理
  -> staging texture
  -> CPU 映射
  -> BGRA buffer
  -> UpdateLayeredWindow
```

优点是实现和窗口兼容性相对直接；缺点是每帧 GPU→CPU 读回，会失去部分 GPU 优势。

适合：中小尺寸宠物、第一版 GPU 试验、需要兼容各种桌面环境的实现。

#### 路线 B：DirectComposition + D3D11 视觉树

使用 DirectComposition 将 D3D11 纹理作为视觉内容提交到窗口。

优点：避免每帧读回 CPU，缩放、透明和合成更接近 GPU 路径；缺点：Win32/DXGI/DirectComposition 资源生命周期更复杂，调试成本更高。

适合：高分辨率、多个宠物、粒子效果、持续变换和高刷新率场景。

建议先实现路线 A 或软件后端，再把 `Renderer` 换成路线 B；不要让 DirectComposition 的细节扩散到动画和状态机代码。

## Rust 与 C++ 选择

### Rust

适合：

- 希望资源生命周期和线程安全更严格；
- 希望把动画状态、资源缓存和输入状态写成可测试模块；
- 接受通过 Windows crate/COM 接口处理 Win32 和 DirectX。

推荐结构：

```text
src/
  window/       Win32 窗口过程、DPI、输入
  animation/    clip、player、clock
  asset/        解码、缓存、纹理上传
  render/       renderer trait、software、d3d11
  pet/          状态机和交互
```

`unsafe` 应集中在 `window` 和 `render/d3d11` 模块，不能让业务状态机到处调用 Win32。

### C++

适合：

- 需要最直接地使用 Win32、Direct2D、DirectComposition 和 Windows SDK；
- 团队已有 Windows 图形开发经验；
- 需要接入现有 C++ 图形工具链。

建议使用 RAII 封装 `HWND`、DC、DIB、COM 接口和 D3D 资源。窗口过程只做消息分发，不要在 `WndProc` 中直接实现动画逻辑。

### 推荐决策

如果项目本身已经是 Rust，建议 Rust + Win32 API + D3D11。先做软件分层窗口后端，再增加 D3D 后端。

如果目标是最快得到一个稳定的 Windows 原型，C++ + Win32 + D3D11/Direct2D 会更直接。

两种语言都不建议引入 GUI 框架，因为目标项目已经明确要求 GUI 直接使用 Win32 API。

## 线程模型

推荐三个角色，但第一版可以合并其中两个：

```text
UI/message thread
  负责 GetMessage/DispatchMessage、窗口位置和输入

Asset worker
  负责磁盘读取、图片解码、缓存准备

Render/update thread
  负责动画时间推进、绘制和窗口提交
```

关键原则：

- `WndProc` 不阻塞等待图片加载；
- 资源加载完成通过消息或无锁/低锁队列通知；
- 窗口句柄只在拥有它的线程上操作，除非明确遵守 Win32 约束；
- 渲染提交要有明确的所有权和生命周期；
- 关闭窗口时先停止更新，再释放 GPU/位图资源，最后销毁窗口。

## DPI、多显示器和坐标

启动时调用 `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`，并处理 `WM_DPICHANGED`。

必须区分：

- 逻辑坐标：动画和角色状态使用；
- 物理像素：DIB、纹理和窗口客户区使用；
- 屏幕坐标：`SetWindowPos` 使用。

窗口移动、命中测试和渲染尺寸都要使用同一个 DPI 快照，否则会出现角色偏移、鼠标位置不准和跨屏缩放错误。

## MVP 实施顺序

### 阶段 1：窗口验证

- 创建无边框 `WS_POPUP` 窗口；
- 设置 `WS_EX_LAYERED | WS_EX_TOOLWINDOW`；
- 使用 CPU BGRA buffer 调用 `UpdateLayeredWindow`；
- 显示一张带半透明边缘的 PNG；
- 验证置顶、移动、关闭和多显示器。

### 阶段 2：动画验证

- 加载一组 PNG 帧；
- 使用 `Instant`/QPC 更新动画；
- 支持循环和停止；
- 只在帧变化时提交窗口；
- 加入双缓冲，避免读写同一块像素缓冲。

### 阶段 3：交互验证

- 鼠标命中区域；
- 拖拽移动；
- 点击和长按；
- 动态开启/关闭穿透；
- 不抢前台焦点。

### 阶段 4：DirectX 后端

- 初始化 D3D11；
- 将帧上传为 shader resource；
- 用全屏/矩形 quad 绘制到离屏目标；
- 验证 alpha 预乘和颜色一致性；
- 再评估 DirectComposition 是否值得替换 `UpdateLayeredWindow`。

## 测试策略

可在不创建窗口的情况下测试大多数行为：

- 帧时间累计和帧索引选择；
- 循环、暂停、停止和结束回调；
- 位置边界和跨屏坐标换算；
- hitbox 与 alpha 命中规则；
- 资源缓存淘汰；
- 预乘 alpha 转换。

窗口和 DirectX 只需要少量集成测试：

- 能否创建和销毁透明窗口；
- `UpdateLayeredWindow` 是否正确显示半透明 PNG；
- 置顶/非置顶切换；
- 穿透切换；
- DPI 改变后的窗口尺寸和位置。

## 性能预算与观测

第一版应记录：

- 单帧 CPU 更新时间；
- 单帧 GPU 时间；
- 窗口提交耗时；
- 资源解码耗时；
- 纹理/位图缓存大小；
- 实际帧间隔和丢帧次数。

如果一个 500×500 的单角色动画已经明显占用 CPU，应先检查是否每帧重复解码、重复分配或重复提交；不要直接把所有逻辑迁移到 D3D12。

## 建议的第一版结论

对于这个功能，推荐的起点是：

```text
Rust
  + 直接 Win32 API
  + WS_EX_LAYERED 分层窗口
  + UpdateLayeredWindow
  + CPU BGRA 双缓冲
  + Instant 驱动动画时钟
  + PNG 精灵图/帧缓存
```

完成并验证窗口行为后，再添加：

```text
D3D11 texture
  + Direct2D 或简单 quad 绘制
  + DirectComposition（仅在 profiling 证明需要时）
```

这条路线的核心价值是：窗口、动画、资源和渲染器之间有清晰的 seam；以后无论使用 Rust/C++、CPU/D3D11，还是从单个 Pet 扩展到多个桌面角色，业务逻辑都不需要跟着重写。

