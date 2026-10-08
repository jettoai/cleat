<h1 align="center">Cleat</h1>
<p align="center"><sub>by <a href="https://jetto.ai">Jetto</a></sub></p>

<p align="center">让 Mac 的音频设备待在你放好的位置：<br>声音从对的扬声器出来、对的麦克风停在对的音量，AirPods 从手机那边要回来。</p>

<p align="center"><a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · <b>简体中文</b> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a></p>

Cleat 是船上系缆绳的羊角桩，绑上去船就不会漂走。这一个是给音频设备用的：你说要哪个输出、哪个麦克风、多大音量、怎样的左右平衡，Cleat 就把它固定住。它由 CoreAudio 事件驱动、不轮询，几乎不占 CPU；平时待在菜单栏，所有固定的项目都能在设置窗口里调整。

macOS 总是把音频设备换掉。连上 AirPods Max，麦克风就被它抢走（蓝牙也跟着降成通话音质的 HFP）。视频会议应用会“自动调整麦克风音量”，调完就停在别的值。虚拟机一启动就把输出音量改掉。重新连接几次后，左右平衡偏到一边。被手机借走的 AirPods 留在手机那边，Mac 接下来一整天都从扬声器出声。发射器关掉的无线接收器，在 CoreAudio 看来仍是一个正常设备，只是发出来的全是静音。

**Cleat 做的事**

- **输出优先级。** 声音从列表中第一个已连接的设备出来。只要列表上有设备连着，标为“不使用”的设备就算 macOS 自己切过去，也会被移开。
- **麦克风优先级。** 列表中第一个已连接的麦克风就是默认输入；列表上有麦克风连着时，屏蔽列表让 AirPods Max（或 Zoom、Teams 的虚拟设备）进不了这个位置。
- **蓝牙耳机连上就接管。** 耳机一连上，声音就切过去，和有线耳机一样。
- **把 AirPods 从手机要回来。** 你列出的耳机已连上，音频却在手机或 iPad 那边，而 Mac 正在播放、你也在 Mac 前面时，Cleat 会把它要回来。手机真的在播放或在通话，就让手机留着。耳机因为还没戴上而拒绝时，只要 Mac 还在播放，Cleat 每 8 秒再要一次，最多 3 分钟。
- **固定麦克风音量。** 所有麦克风一个值，个别设备可以单独设置。
- **挡住你指定的程序改输出音量。** Parallels Desktop（或你添加的任何应用）改了输出音量，Cleat 会改回去。你自己调的会保留。
- **固定左右平衡**，停在居中或你设置的位置。
- **完全静音的设备视为不存在。** 发出纯数字静音的接收器会被跳过，由列表中下一个麦克风接管。
- **你自己的选择不去动。** 你亲手选、又不在两份列表上的麦克风，会一直保持你的选择。
- **设置窗口与菜单栏图标**，显示当前在用哪个设备，以及 Cleat 本身占用多少 CPU 和内存。
- **错误报告要你开启才发送。** 默认关闭。关闭时，还在排队的报告会删除，发送到一半的会中止。

<p align="center">
  <img src="assets/screenshot-output.png" alt="Cleat 设置窗口的“输出”页，深色模式、繁体中文界面：左侧栏有输出、输入、耳机三页；顶部状态卡显示 CPU、内存和拉回速度；输出优先级第一个是“外接耳機”（外接耳机）并标着使用中；下方其他输出设备里，Mac Studio 的扬声器和 Maono AI Microphone 勾了不使用；最下面的输出音量与平衡卡片，对 Parallels Desktop 开启了音量拉回，左右平衡设为固定，并注明当前的输出“外接耳機”不支持左右平衡，所以 Cleat 不动它" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-input.png" alt="Cleat 设置窗口的“输入”页，深色模式、繁体中文界面：输入优先级依次是 Wireless microphone（使用中）、Brio 100、AirPods Max（未连接、已排除）；其他输入设备中 Microsoft Teams Audio 和 ZoomAudioDevice 已排除；输入音量卡片把所有麦克风固定在 100%，当前读数是 Wireless microphone 100%" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-headphones.png" alt="Cleat 设置窗口的“耳机”页，深色模式、繁体中文界面：“蓝牙耳机连上时自动切过去”和“耳机被其他设备拿走时要回来”两个开关都已打开；AirPods Max 和 AirPods Pro（未连接）已勾选；下方是折叠的“其他蓝牙设备（3）”" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-output-light.png" alt="同一个“输出”页的浅色模式，繁体中文界面" width="720">
</p>

<p align="center"><sub>设置窗口目前只有繁体中文界面。配置文件和 <code>cleat</code> 命令是英文。</sub></p>

## 它固定哪些东西

其中六条规则形状相同：订阅一个 CoreAudio 属性，和配置文件比对，不同就写回去。输出音量那条还要知道是哪个程序改的，CoreAudio 不提供这项信息，所以从系统日志里读出来。要回耳机那条则是向另一个系统守护进程发出请求，因为被手机拿走的耳机并不是一个能直接写的 CoreAudio 属性。

| | 功能 | 配置 |
|---|---|---|
| 1 | 输入设备优先级，附屏蔽列表 | `input`、`blockedInput` |
| 2 | 发出纯数字静音的设备视为不存在 | `liveness` |
| 3 | 固定输出的左右平衡 | `balance` |
| 4 | 输出设备优先级，附屏蔽列表 | `output`、`blockedOutput` |
| 5 | 逐个固定输入设备的音量 | `inputVolume` |
| 6 | 蓝牙耳机连上时接管输出 | `headphonesTakeOver` |
| 7 | 耳机被手机拿走时要回来 | `reclaim`、`reclaimEnabled` |
| 8 | 撤销你指定的程序对输出音量的修改 | `outputVolumeHoldAgainst`、`outputVolumeHoldEnabled` |

你自己挑的设备，它不会抢走。当前的默认输入如果既不在优先级上、也不在屏蔽列表上（例如你在“系统设置”里选的麦克风，或 Zoom、Teams 的虚拟设备），Cleat 就不去动它。

## 设置窗口与菜单栏

守护进程会在菜单栏放一个图标。菜单上显示当前使用的输出与输入，可以打开设置窗口、显示“关于”，以及退出 Cleat。

设置窗口有三页：输出、输入、耳机。每一页顶部是一张状态卡：Cleat 是否在运行；它的 CPU（一个核心满载算 100%，取最近一分钟的平均，与“活动监视器”算法相同）；内存（与“活动监视器”的“内存”列同一个值）；以及拉回速度（设置被其他东西改掉后，Cleat 改回来要多久，取最近几次的中位数）。

- **输出**：优先级列表；其他每一个输出设备，各带“加入顺序”按钮和“不使用”复选框；改了音量会被拉回的程序名单（附最近一次拉回的时间）；左右平衡与实时读数。
- **输入**：麦克风优先级列表；其他每一个输入设备；音量：所有麦克风共用一个滑块，另外每添加一个设备多一个滑块。
- **耳机**：自动切换开关、要回耳机开关，以及每一副配对过的蓝牙耳机的复选框。系统没有识别为音频设备的蓝牙设备收在一个折叠的分组里，给不报自己身份的音箱或耳机用。

在优先级列表的设备上点右键，可以上移或下移。每次修改都会在片刻后写进配置文件，守护进程再从那里读到；窗口没显示的键完全原样保留。窗口打开期间配置文件在磁盘上被改过，窗口就停止写入，并提供“重新载入”。

打开方式：菜单栏图标、`cleat settings`，或在守护进程运行时从 Finder、Spotlight、Raycast 打开 Cleat.app。设置窗口同一时间只会打开一个。

## 安装

```sh
brew tap jettoai/tap
# Homebrew requires third-party taps to be trusted before it will load their casks.
brew trust jettoai/tap
brew install --cask cleat
```

或从 [Releases](https://github.com/jettoai/cleat/releases) 下载 zip，解压到 `/Applications`，打开一次。

接着写一份配置文件并启动：

**复制示例之前先读一遍。** 它是作者自己的配置，并非中性的起点：它把每个输入设备的音量固定在 100%（`"inputVolume": {"*": 100, ...}`），也开启了耳机自动切换，任何蓝牙耳机一连上就会变成输出。里面的设备名称是作者的，所以指定设备的规则在你的机器上什么都不会做，直到你换成自己的设备。有两项设置不指定任何设备、会立即生效：`balance` 会把你当前的输出拉回居中，`launchAtLogin` 会把 Cleat 注册为登录项。先改成你的设备，或从 `{}` 开始一次加一条规则；也可以交给设置窗口来编辑。

```sh
mkdir -p ~/.config/cleat
cp /Applications/Cleat.app/Contents/Resources/config.example.json ~/.config/cleat/config.json
cleat restart
```

`cleat restart` 会注册应用内附带的 launchd agent，并通过它启动守护进程；之后 launchd 会在登录时启动 Cleat，被杀掉或崩溃时也会再把它启动。正常退出（菜单里的“退出 Cleat”，或 `brew upgrade` 替换旧版本）是有意不重启的；Cleat 会在下次登录、你再次打开应用，或执行 `cleat restart` 时回来。`cleat status` 会显示 agent 当前处于哪个状态。把 `launchAtLogin` 设为 `false` 会取消注册 agent，Cleat 也会随之退出。

改用手动安装的话：从 Finder 打开应用一次，这既是第一次请求麦克风权限，也是它注册 agent 的时刻。agent 运行时再打开应用，打开的是设置窗口，所以永远不会有第二个守护进程。

## 配置文件

`~/.config/cleat/config.json`，保存后一秒内重新读取：

```json
{
  "input": ["Wireless microphone", "Brio 100"],
  "blockedInput": ["AirPods Max"],
  "output": ["外接耳機", "Mac Studio的揚聲器"],
  "blockedOutput": ["Maono AI Microphone"],
  "headphonesTakeOver": true,
  "balance": 0.5,
  "inputVolume": { "*": 100, "Wireless microphone": 88, "Brio 100": 75 },
  "liveness": { "Wireless microphone": { "zeroSeconds": 3 } },
  "reclaim": ["AirPods Max"],
  "outputVolumeHoldAgainst": ["Parallels Desktop"],
  "launchAtLogin": true,
  "errorReports": false
}
```

| 字段 | 类型 | 默认值 | 含义 |
|---|---|---|---|
| `input` | 字符串数组 | `[]` | 输入优先级，最想要的放最前面。为空则关闭这条规则 |
| `blockedInput` | 字符串数组 | `[]` | `input` 列表上有设备连着时，永远不能成为默认输入 |
| `output` | 字符串数组 | `[]` | 输出优先级。为空则关闭这条规则 |
| `blockedOutput` | 字符串数组 | `[]` | `output` 列表上有设备连着时，永远不能成为默认输出 |
| `headphonesTakeOver` | 布尔 | `false` | 蓝牙输出设备连上时接管输出 |
| `balance` | 数字或 null | `null` | 0.0（左）到 1.0（右），0.5 为居中。`null` 关闭这条规则 |
| `inputVolume` | 对象 | `{}` | 设备名称（或 `"*"` 表示所有输入设备）对应百分比，0 到 100 |
| `liveness` | 对象 | `{}` | 设备名称对应 `{ "zeroSeconds": N }`，N 至少为 1 |
| `reclaim` | 字符串数组 | `[]` | 被其他设备拿走时，Cleat 要拿回来的蓝牙耳机，可用名称或地址 |
| `reclaimEnabled` | 布尔 | `true` | 不清空 `reclaim` 也能关闭要回耳机 |
| `outputVolumeHoldAgainst` | 字符串数组 | `[]` | 改了输出音量会被撤销的程序 |
| `outputVolumeHoldEnabled` | 布尔 | `true` | 不清空名单也能关闭音量拉回 |
| `launchAtLogin` | 布尔 | `true` | 注册 launchd agent，登录时启动 Cleat、进程退出时重新启动 |
| `errorReports` | 布尔 | `false` | 把崩溃和错误报告发送到 Sentry。见[隐私](#隐私) |

**输入音量。** `"*"` 设置当前所有输入设备的目标值，指定的项目会覆盖该设备的值：`{"*": 100, "Brio 100": 75}` 把所有设备固定在 100%，只有 Brio 固定在 75。没有 `"*"` 时，配置文件未指定的设备不会被动。通配符也覆盖被屏蔽的设备，所以被 `blockedInput` 挡在输入位置外的 AirPods Max，音量照样会被固定；读不到音量的设备（某些虚拟设备）则无论如何都不动。

**耳机。** 蓝牙输出设备一出现，就成为输出。耳机还连着的时候你手动选了别的设备，Cleat 会尊重：开启 `headphonesTakeOver` 时，`output` 永远不会把声音从已连接的蓝牙设备移走，也不会移到它上面，除非该设备在 `blockedOutput` 上。那份列表的意思是“绝对不要这个”，优先于“这是耳机”。所以没有耳机占着输出时，由优先级决定从哪里出声。macOS 对有线耳机本来就这样做，iOS 对 AirPods 也是；但在 Mac 上用蓝牙，重新连上一副最后配对在手机上的耳机时，声音仍从扬声器出来，这正是 Cleat 补上的缺口。Cleat 启动时已经连着的耳机不算刚连上，所以重启 Cleat 永远不会换掉输出。

`blockedOutput` 是另一半。有些 USB 麦克风带有扬声器端，耳机一离开，macOS 就落到那里。它不在你的优先级上，如果没有屏蔽列表，Cleat 会把它当成你自己选的输出而不去动。只要 `output` 上有设备连着，被屏蔽的设备一定会被移开，即使优先级上没有地方可以送：当列表上每个设备都是 Cleat 不能碰的耳机时，声音会送到第一个既不在屏蔽列表、也不是耳机的输出；只有找不到这样的设备时才留在原处。

**要回耳机。** 同时配对 Mac 和手机的 AirPods，属于最后一个请求它的设备。手机播放东西就等于请求；播完之后，Mac 这边没有任何东西再请求一次，耳机就一直留在手机那边（蓝牙还连着，却从音频设备列表上消失），Mac 接下来一整天都从扬声器出声。macOS 只在这边有应用“开始”播放时才把耳机拿回来，而那时声音早已从别处出去了。

把耳机列进 `reclaim`，Cleat 就会把它要回来。Cleat 发出的路由请求与 macOS 自己发的相同，由耳机根据两台设备各自在做什么来决定：请求表示这台 Mac 有一个播放会话，分数高于闲置的手机，低于正在播放媒体或通话中的手机。所以闲置的手机会让出耳机，真正在用的手机会留着。发出请求前要同时满足五个条件：耳机在列表上、它连着这台 Mac、它在这台 Mac 上没有音频设备、当前的输出正在播放，并且有人在 Mac 前面（最近 30 秒内动过键盘或鼠标，或最前面的应用正在播放视频）。被会议或 `caffeinate` 保持常亮的屏幕不算有人。你手动把输出从耳机移走，这一轮播放期间都会被尊重。手机留住的耳机，一分钟内不会再请求；一分钟后，要等下一次 Cleat 会响应的音频变动（例如开始播放或有设备连接）才会再请求。日志里也只记一次，不会每拍都记。请求被接受时，日志会写出耳机花了多久才成为输出，或写出它没有回来。

耳机除了用名称，也可以用蓝牙地址指定（`"70:F9:4A:B6:0C:C9"`，接受连字符和小写），两副同名耳机就靠这个区分。`cleat reclaim` 会手动发出一次请求并打印回应，用来查看仲裁的结果。这条规则使用系统私有接口：在没有这个接口的 macOS 上，规则会自行关闭，在日志里说明一次，`cleat status` 也会显示“不可用”而非“开启”。

**输出音量。** CoreAudio 只告诉你音量变了，没说是谁改的，所以 Cleat 从系统的音频日志里读出修改者。名单（`outputVolumeHoldAgainst`）上的程序改的，会被改回修改前的值；你自己改的，或名单外的程序改的，就成为新的固定值。名单项可以是可执行文件名或应用名：`Parallels Desktop` 会匹配 `Parallels Desktop.app` 里的任何程序。

**指定设备。** 使用“系统设置”里显示的名称；两个设备同名时，改用 CoreAudio UID。名称比对前会先做 Unicode 规范化，因为有些设备名称里带着打不出来的不换行空格（例如 Maono 的）。大小写不做规范化。

格式错误或数值超出范围的配置文件，永远不会替换可用的配置：之前的设置继续生效，`cleat status` 会说明原因。还没加载任何配置前，所有规则都是关闭的。删除配置文件也会关闭所有规则，这是不必退出 Cleat 就让它停手的方法。

**静音检测**（`liveness`）是唯一会打开麦克风的功能。Cleat 在指定设备上运行 HAL IOProc，检查缓冲区里每一个样本是否都恰好为零：真正的麦克风一定有底噪，发射器关掉的接收器什么都不发。这种状态持续 `zeroSeconds` 秒后，该设备就视为不存在，由优先级上的下一个设备接管。因为输入被打开，Cleat 监视期间 macOS 会显示橙色的麦克风圆点。

## 命令

应用本身就是 CLI，Homebrew cask 把它链接为 `cleat`。

```sh
cleat status      # what it is holding right now, and why
cleat log -n 50   # recent events
cleat restart     # start the daemon, or replace the running one, through its launchd agent
cleat reclaim     # ask for the headsets under "reclaim", once, and print the answer
cleat settings    # open the settings window
cleat version
```

Cleat 没在运行时，用 `cleat restart` 就对了：还没注册 agent 就先注册，再让 launchd 换上新的进程。其他什么都不需要手动启动。

`cleat status` 读取 `~/Library/Application Support/Cleat-rs/status.json`；`cleat log` 读取 `~/Library/Logs/Cleat-rs/cleat-rs.log`。这些命令读的是守护进程写出来的文件，和它之间没有任何直接通信。只有真的改动了东西的操作才会记进日志，所以日志安静代表这一天很平静，守护进程并没有坏；要确认它活着，看 `status`。

## 麦克风权限

只有 `liveness` 需要。Cleat 第一次运行时 macOS 会请求；拒绝的话，其他规则照常工作，`cleat status` 显示 `microphone: denied`。想改变主意：“系统设置”>“隐私与安全性”>“麦克风”，然后执行 `cleat restart`。Cleat 只在启动时读取权限，不会监视那个开关。

这也是 Cleat 做成 .app、而非挂在 LaunchAgent 上的单个可执行文件的原因：由 launchd 启动的命令行工具常常根本不会被请求权限，请求就悄无声息地失败。监管守护进程的 agent 启动的是应用自己的可执行文件（`BundleProgram`），所以 launchd 拉起来的进程，就是当初获得麦克风权限的那个应用。

## 隐私

除非你要求，Cleat 不会把任何东西发出你的 Mac。唯一的例外需要你主动开启：在配置文件里设置 `"errorReports": true`，守护进程就会把崩溃和错误报告发送到 Sentry，让崩溃无需有人反馈也能传到作者手上。报告内容包括堆栈跟踪或错误信息、Cleat 和 macOS 的版本，以及时间。报告里没有 IP 地址、没有用户信息、没有你的配置、也没有任何文件内容，文件路径中的主目录会替换成 `~`。Sentry 和任何服务器一样，能看到报告到达时的连接。没有使用跟踪、没有会话跟踪、也没有屏幕录制。

改回 `false` 或删掉那一行，配置文件重新读取后就立即停止上报，无需重启。还在排队的报告会删除，发送到一半的会中止。`cleat status` 会显示当前的设置。`cleat` 命令和设置窗口永远不会发送任何东西。

## 从源码构建

```sh
cd rs
cargo build --release
cargo test
bash scripts/bundle.sh    # target/bundle.noindex/Cleat.app
```

`scripts/bundle.sh` 会构建出 ad-hoc 签名、使用开发身份 `ai.jetto.cleat.rs` 的应用，这个身份永远不会注册登录 agent；要在 launchd 下测试，做法见 `rs/README.md`。

## 卸载

```sh
brew uninstall --cask --zap cleat
```

手动安装的话：用 `launchctl bootout gui/$UID/ai.jetto.cleat` 卸载 agent，删除 `/Applications/Cleat.app`，再移除 `~/Library/Application Support/Cleat-rs`、`~/Library/Logs/Cleat-rs` 和 `~/.config/cleat`。

## 许可证

MIT
