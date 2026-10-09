<h1 align="center">Cleat</h1>
<p align="center"><sub>by <a href="https://jetto.ai">Jetto</a></sub></p>

<p align="center">讓 Mac 的音訊裝置待在你放好的位置：<br>聲音從對的喇叭出來、對的麥克風停在對的音量，AirPods 從手機那邊要回來。</p>

<p align="center"><a href="README.md">English</a> · <b>繁體中文</b> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a></p>

Cleat 是船上繫纜繩的羊角樁，綁上去船就不會漂走。這一個是給音訊裝置用的：你說要哪個輸出、哪支麥克風、多少音量、怎樣的左右平衡，Cleat 就把它固定住。它靠 CoreAudio 事件驅動、不輪詢，幾乎不吃 CPU；平常待在選單列，所有固定的項目都能在設定視窗裡調整。

macOS 老是把音訊裝置換掉。連上 AirPods Max，麥克風就被它搶走（藍牙也跟著降成通話品質的 HFP）。視訊會議 app 會「自動調整麥克風音量」，調完就停在別的值。虛擬機一啟動就把輸出音量改掉。重新連線幾次後，左右平衡偏到一邊。被手機借走的 AirPods 留在手機那邊，Mac 接下來一整天都從喇叭出聲。發射器關掉的無線接收器，在 CoreAudio 眼中仍是一個正常裝置，只是送出來的全是無聲。

**Cleat 做的事**

- **輸出優先順序。** 聲音從清單中第一個已連線的裝置出來。只要清單上有裝置連著，標成「不使用」的裝置就算 macOS 自己切過去，也會被移開。
- **麥克風優先順序。** 清單中第一支已連線的麥克風就是預設輸入；清單上有麥克風連著時，封鎖清單讓 AirPods Max（或 Zoom、Teams 的虛擬裝置）進不了這個位置。
- **藍牙耳機連上就接手。** 耳機一連上，聲音就切過去，跟有線耳機一樣。
- **把 AirPods 從手機要回來。** 你列出的耳機已連上，音訊卻在手機或 iPad 那邊，而 Mac 正在播、你也在 Mac 前面時，Cleat 會把它要回來。手機真的在播或在通話，就讓手機留著。耳機因為還沒戴上而拒絕時，只要 Mac 還在播，Cleat 每 8 秒再要一次，最多 3 分鐘。
- **固定麥克風音量。** 所有麥克風一個值，個別裝置可以另外設定。
- **擋住你指定的程式改輸出音量。** Parallels Desktop（或你加入的任何 app）改了輸出音量，Cleat 會改回去。你自己調的會保留。
- **固定左右平衡**，停在置中或你設定的位置。
- **全無聲的裝置當作不在。** 送出完全數位無聲的接收器會被跳過，由清單中下一支麥克風接手。
- **你自己的選擇不去動。** 你親手選、又不在兩份清單上的麥克風，會一直維持你的選擇。
- **設定視窗與選單列圖示**，顯示目前用哪個裝置，以及 Cleat 本身用掉多少 CPU 與記憶體。
- **錯誤回報要你開才送。** 預設關閉。關掉時，還在排隊的報告會刪掉，送到一半的會中止。

<p align="center">
  <img src="assets/screenshot-output.png" alt="Cleat 設定視窗的「輸出」頁，深色模式、繁體中文介面：左側欄有輸出、輸入、耳機三頁；上方狀態卡顯示 CPU、記憶體與拉回速度；輸出優先順序第一個是「外接耳機」並標著使用中；下方其他輸出裝置裡，Mac Studio的揚聲器與 Maono AI Microphone 勾了不使用；最下面的輸出音量與平衡卡片，對 Parallels Desktop 開著音量拉回，左右平衡設為固定，並註明目前的輸出「外接耳機」不支援左右平衡，所以 Cleat 不動它" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-input.png" alt="Cleat 設定視窗的「輸入」頁，深色模式、繁體中文介面：輸入優先順序依序是 Wireless microphone（使用中）、Brio 100、AirPods Max（未連線、已排除）；其他輸入裝置中 Microsoft Teams Audio 與 ZoomAudioDevice 已排除；輸入音量卡片把所有麥克風固定在 100%，目前讀數是 Wireless microphone 100%" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-headphones.png" alt="Cleat 設定視窗的「耳機」頁，深色模式、繁體中文介面：「藍牙耳機連上時自動切過去」與「耳機被其他裝置拿走時要回來」兩個開關都打開；AirPods Max 與 AirPods Pro（未連線）已勾選；下方是收合的「其他藍牙裝置（3）」" width="720">
</p>

<p align="center">
  <img src="assets/screenshot-output-light.png" alt="同一個「輸出」頁的淺色模式，繁體中文介面" width="720">
</p>

<p align="center"><sub>設定視窗目前只有繁體中文介面。設定檔與 <code>cleat</code> 指令是英文。</sub></p>

## 它固定哪些東西

其中六條規則形狀相同：訂閱一個 CoreAudio 屬性，和設定檔比對，不同就寫回去。輸出音量那條還要知道是哪個程式改的，CoreAudio 不提供這項資訊，所以從系統 log 讀出來。要回耳機那條則是向另一個系統常駐程式提出請求，因為被手機拿走的耳機並不是一個能直接寫的 CoreAudio 屬性。

| | 功能 | 設定 |
|---|---|---|
| 1 | 輸入裝置優先順序，附封鎖清單 | `input`、`blockedInput` |
| 2 | 送出完全數位無聲的裝置當作不在 | `liveness` |
| 3 | 固定輸出的左右平衡 | `balance` |
| 4 | 輸出裝置優先順序，附封鎖清單 | `output`、`blockedOutput` |
| 5 | 逐一固定輸入裝置的音量 | `inputVolume` |
| 6 | 藍牙耳機連上時接手輸出 | `headphonesTakeOver` |
| 7 | 耳機被手機拿走時要回來 | `reclaim`、`reclaimEnabled` |
| 8 | 撤銷你指定的程式對輸出音量的修改 | `outputVolumeHoldAgainst`、`outputVolumeHoldEnabled` |

你自己挑的裝置，它不會搶走。目前的預設輸入如果既不在優先順序上、也不在封鎖清單上（例如你在「系統設定」選的麥克風，或 Zoom、Teams 的虛擬裝置），Cleat 就不去動它。

## 設定視窗與選單列

常駐程式會在選單列放一個圖示。選單上顯示目前使用中的輸出與輸入，可以開啟設定視窗、顯示「關於」，以及結束 Cleat。

設定視窗有三頁：輸出、輸入、耳機。每一頁最上方是一張狀態卡：Cleat 有沒有在執行；它的 CPU（一顆核心滿載算 100%，取最近一分鐘的平均，與「活動監視器」同一個算法）；記憶體（與「活動監視器」的「記憶體」欄同一個值）；以及拉回速度（設定被其他東西改掉後，Cleat 改回來要多久，取最近幾次的中位數）。

- **輸出**：優先順序清單；其他每一個輸出裝置，各附「加入順序」按鈕與「不使用」勾選框；改了音量會被拉回的程式名單（附最近一次拉回的時間）；左右平衡與即時讀數。
- **輸入**：麥克風優先順序清單；其他每一個輸入裝置；音量：所有麥克風共用一條滑桿，另外每加一個裝置多一條滑桿。
- **耳機**：自動切換開關、要回耳機開關，以及每一副配對過的藍牙耳機的勾選框。系統沒辨識成音訊裝置的藍牙裝置收在一個收合的群組裡，給沒有自報身分的喇叭或耳機用。

在優先順序清單的裝置上按右鍵，可以上移或下移。每個修改都會在片刻後寫進設定檔，常駐程式再從那裡讀到；視窗沒顯示的鍵完全照原樣保留。視窗開著時設定檔在磁碟上被改過，視窗就停止寫入，並提供「重新載入」。

開啟方式：選單列圖示、`cleat settings`，或在常駐程式執行時從 Finder、Spotlight、Raycast 打開 Cleat.app。設定視窗同一時間只會開一個。

## 安裝

```sh
brew tap jettoai/tap
# Homebrew requires third-party taps to be trusted before it will load their casks.
brew trust jettoai/tap
brew install --cask cleat
```

或從 [Releases](https://github.com/jettoai/cleat/releases) 下載 zip，解壓到 `/Applications`，打開一次。

接著寫一份設定檔並啟動：

**複製範例之前先讀過一遍。** 它是作者自己的設定，並非中性的起點：它把每個輸入裝置的音量固定在 100%（`"inputVolume": {"*": 100, ...}`），也開啟了耳機自動切換，任何藍牙耳機一連上就會變成輸出。裡面的裝置名稱是作者的，所以指名裝置的規則在你的機器上什麼都不會做，直到你換成自己的裝置。有兩項設定不指名任何裝置、會立刻生效：`balance` 會把你目前的輸出拉回置中，`launchAtLogin` 會把 Cleat 註冊成登入項目。先改成你的裝置，或從 `{}` 開始一次加一條規則；也可以交給設定視窗來編輯。

```sh
mkdir -p ~/.config/cleat
cp /Applications/Cleat.app/Contents/Resources/config.example.json ~/.config/cleat/config.json
cleat restart
```

`cleat restart` 會註冊 app 內附的 launchd agent，並透過它啟動常駐程式；之後 launchd 會在登入時啟動 Cleat，被殺掉或當掉時也會再把它啟動。正常結束（選單的「結束 Cleat」，或 `brew upgrade` 換掉舊版）是刻意不重啟的；Cleat 會在下次登入、你再次打開 app，或執行 `cleat restart` 時回來。`cleat status` 會顯示目前 agent 處於哪個狀態。把 `launchAtLogin` 設成 `false` 會取消註冊 agent，Cleat 也會跟著結束。

改用手動安裝的話：從 Finder 打開 app 一次，這同時是第一次詢問麥克風權限、也是它註冊 agent 的時候。agent 執行中再打開 app，開的是設定視窗，所以永遠不會有第二個常駐程式。

## 設定檔

`~/.config/cleat/config.json`，存檔後一秒內重新讀取：

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

| 欄位 | 型別 | 預設值 | 意思 |
|---|---|---|---|
| `input` | 字串陣列 | `[]` | 輸入優先順序，最想要的放最前面。空的就關閉這條規則 |
| `blockedInput` | 字串陣列 | `[]` | 永遠不能成為預設輸入。移到哪裡見[不使用](#not-used) |
| `output` | 字串陣列 | `[]` | 輸出優先順序。空的就關閉這條規則 |
| `blockedOutput` | 字串陣列 | `[]` | 永遠不能成為預設輸出。移到哪裡見[不使用](#not-used) |
| `headphonesTakeOver` | 布林 | `false` | 藍牙輸出裝置連上時接手輸出 |
| `balance` | 數字或 null | `null` | 0.0（左）到 1.0（右），0.5 是置中。`null` 關閉這條規則 |
| `inputVolume` | 物件 | `{}` | 裝置名稱（或 `"*"` 代表所有輸入裝置）對應百分比，0 到 100 |
| `liveness` | 物件 | `{}` | 裝置名稱對應 `{ "zeroSeconds": N }`，N 至少 1 |
| `reclaim` | 字串陣列 | `[]` | 被其他裝置拿走時，Cleat 要拿回來的藍牙耳機，可用名稱或位址 |
| `reclaimEnabled` | 布林 | `true` | 不清空 `reclaim` 也能關掉要回耳機 |
| `outputVolumeHoldAgainst` | 字串陣列 | `[]` | 改了輸出音量會被撤銷的程式 |
| `outputVolumeHoldEnabled` | 布林 | `true` | 不清空名單也能關掉音量拉回 |
| `launchAtLogin` | 布林 | `true` | 註冊 launchd agent，登入時啟動 Cleat、程式掛掉時重新啟動 |
| `errorReports` | 布林 | `false` | 把當機與錯誤報告送到 Sentry。見[隱私](#隱私) |

**輸入音量。** `"*"` 設定目前所有輸入裝置的目標值，指名的項目會覆蓋該裝置的值：`{"*": 100, "Brio 100": 75}` 把所有裝置固定在 100%，只有 Brio 固定在 75。沒有 `"*"` 時，設定檔沒指名的裝置不會被動到。萬用字元也涵蓋被封鎖的裝置，所以被 `blockedInput` 擋在輸入位置外的 AirPods Max，音量照樣會被固定；讀不到音量的裝置（某些虛擬裝置）則無論如何都不動。

**耳機。** 藍牙輸出裝置一出現，就成為輸出。耳機還連著的時候你手動選了別的裝置，Cleat 會尊重：開啟 `headphonesTakeOver` 時，`output` 永遠不會把聲音從已連線的藍牙裝置移走，也不會移到它上面，除非該裝置在 `blockedOutput` 上。那份清單的意思是「絕對不要這個」，優先於「這是耳機」。所以沒有耳機佔著輸出時，由優先順序決定從哪裡出聲。macOS 對有線耳機本來就這樣做，iOS 對 AirPods 也是；但在 Mac 上用藍牙，重新連上一副最後配對在手機的耳機時，聲音仍從喇叭出來，這就是 Cleat 補上的缺口。Cleat 啟動時已經連著的耳機不算剛連上，所以重啟 Cleat 永遠不會換掉輸出。

`blockedOutput` 是另一半。有些 USB 麥克風附帶喇叭端，耳機一離開，macOS 就落到那裡。它不在你的優先順序上，如果沒有封鎖清單，Cleat 會把它當成你自己選的輸出而放著不動。

<a id="not-used"></a>**不使用。** 在 `blockedInput` 或 `blockedOutput` 上的裝置，Cleat 永遠不會選它。macOS 仍把它設為預設時，Cleat 會把它移開：

- 輸入：移到清單上第一個有訊號的麥克風；沒有就移到內建麥克風；再沒有，就移到清單上靜音或仍在量測的麥克風（清單上的麥克風一有訊號，就讓給它）。除了內建麥克風，不在 `input` 上的麥克風永遠不會當作去處。
- 輸出：移到清單上的第一個輸出；沒有就移到任一實體輸出，先內建、再依名稱排序。虛擬裝置、聚合裝置、Continuity 與 AirPlay 裝置，以及由 `headphonesTakeOver` 接手的耳機，永遠不會當作去處。

無處可去時，該裝置留在原地，`status.json` 在 `stuck` 下寫出它的名字，設定視窗也會這樣提示。不是實體裝置的被封鎖裝置（會議軟體的虛擬麥克風、經 Continuity 連上的 iPhone）只是不會被選中：軟體自己切到它時，Cleat 不去動。如果 macOS 一直把被封鎖的裝置放回來，Cleat 在一分鐘內移了三次就停手，等到裝置增減、設定檔重新載入或麥克風訊號變化時再試。

**要回耳機。** 同時配對 Mac 與手機的 AirPods，屬於最後一個要求它的裝置。手機播放東西就等於要求；播完之後，Mac 這邊沒有任何東西再要求一次，耳機就一直留在手機那邊（藍牙還連著，卻從音訊裝置清單上消失），Mac 接下來一整天都從喇叭出聲。macOS 只在這邊有 app「開始」播放時才把耳機拿回來，而那時聲音早已從別的地方出去了。

把耳機列進 `reclaim`，Cleat 就會把它要回來。Cleat 送出的路由請求和 macOS 自己送的一樣，由耳機依兩台裝置各自在做什麼來決定：請求表示這台 Mac 有一個播放工作，分數高於閒置的手機，低於正在播放媒體或通話中的手機。所以閒置的手機會讓出耳機，真的在用的手機會留著。送出請求前要同時滿足五件事：耳機在清單上、它連著這台 Mac、它在這台 Mac 上沒有音訊裝置、目前的輸出正在播放，而且有人在 Mac 前面（最近 30 秒內碰過鍵盤或滑鼠，或最前面的 app 正在播影片）。被會議或 `caffeinate` 撐著不休眠的螢幕不算有人。你手動把輸出從耳機移走，這一輪播放期間都會被尊重。手機保留住的耳機，一分鐘內不會再要；一分鐘後，要等下一次 Cleat 會反應的音訊變動（例如開始播放或有裝置連上）才會再要。log 裡也只記一次，不會每拍都記。請求被接受時，log 會寫出耳機花多久才成為輸出，或寫出它沒有回來。

耳機除了用名稱，也可以用藍牙位址指名（`"70:F9:4A:B6:0C:C9"`，接受連字號與小寫），兩副同名耳機就靠這個區分。`cleat reclaim` 會手動送出一次請求並印出回應，用來看仲裁的結果。這條規則使用系統私有介面：在沒有這個介面的 macOS 上，規則會自己關閉，在 log 裡說一次，`cleat status` 也會顯示「無法使用」而非「開啟」。

**輸出音量。** CoreAudio 只說音量變了，沒說是誰改的，所以 Cleat 從系統的音訊 log 讀出改的人。名單（`outputVolumeHoldAgainst`）上的程式改的，會被改回改之前的值；你自己改的，或名單外的程式改的，就成為新的固定值。名單項目可以是執行檔名稱或 app 名稱：`Parallels Desktop` 會比對到 `Parallels Desktop.app` 裡的任何程式。

**指名裝置。** 使用「系統設定」裡顯示的名稱；兩個裝置同名時，改用 CoreAudio UID。名稱比對前會先做 Unicode 正規化，因為有些裝置名稱裡帶著打不出來的不換行空白（例如 Maono 的）。大小寫不做正規化。

格式錯誤或數值超出範圍的設定檔，永遠不會取代可用的設定：先前的設定繼續生效，`cleat status` 會說明原因。還沒載入任何設定前，所有規則都是關閉的。刪掉設定檔也會關閉所有規則，這是不必結束 Cleat 就讓它停手的方法。

**無聲偵測**（`liveness`）是唯一會打開麥克風的功能。Cleat 在指定裝置上執行 HAL IOProc，檢查緩衝區裡每一個樣本是否都剛好是零：真正的麥克風一定有底噪，發射器關掉的接收器什麼都不送。這種狀態持續 `zeroSeconds` 秒後，該裝置就當作不在，由優先順序上的下一個裝置接手。因為輸入被打開，Cleat 監看期間 macOS 會顯示橘色的麥克風點。

## 指令

app 本身就是 CLI，Homebrew cask 把它連結成 `cleat`。

```sh
cleat status      # what it is holding right now, and why
cleat log -n 50   # recent events
cleat restart     # start the daemon, or replace the running one, through its launchd agent
cleat reclaim     # ask for the headsets under "reclaim", once, and print the answer
cleat settings    # open the settings window
cleat version
```

Cleat 沒在執行時，用 `cleat restart` 就對了：還沒註冊 agent 就先註冊，再讓 launchd 換上新的行程。其他什麼都不需要手動啟動。

`cleat status` 讀的是 `~/Library/Application Support/Cleat/status.json`；`cleat log` 讀的是 `~/Library/Logs/Cleat/cleat.log`。這些指令讀的是常駐程式寫出來的檔案，跟它之間沒有任何直接溝通。只有真的改到東西的動作才會記進 log，所以 log 安靜代表這一天很平靜，常駐程式並沒有壞；要確認它活著，看 `status`。

## 麥克風權限

只有 `liveness` 需要。Cleat 第一次執行時 macOS 會詢問；拒絕的話，其他規則照常運作，`cleat status` 顯示 `microphone: denied`。想改變主意：「系統設定」>「隱私權與安全性」>「麥克風」，然後執行 `cleat restart`。Cleat 只在啟動時讀取權限，不會監看那個開關。

這也是 Cleat 做成 .app、而非掛在 LaunchAgent 上的單一執行檔的原因：由 launchd 啟動的命令列工具常常根本不會被詢問，請求就無聲地失敗。監管常駐程式的 agent 啟動的是 app 自己的執行檔（`BundleProgram`），所以 launchd 拉回來的行程，就是當初取得麥克風權限的那個 app。

## 隱私

除非你要求，Cleat 不會把任何東西送出你的 Mac。唯一的例外需要你主動開啟：在設定檔裡設 `"errorReports": true`，常駐程式就會把當機與錯誤報告送到 Sentry，讓當機不必有人回報也能傳到作者手上。報告內容包括堆疊追蹤或錯誤訊息、Cleat 與 macOS 的版本，以及時間。報告裡沒有 IP 位址、沒有使用者資料、沒有你的設定、也沒有任何檔案內容，檔案路徑中的家目錄會換成 `~`。Sentry 和任何伺服器一樣，看得到報告送達時的連線。沒有使用追蹤、沒有工作階段追蹤、也沒有螢幕錄影。

改回 `false` 或刪掉那一行，設定檔重新讀取後就立刻停止回報，不必重新啟動。還在排隊的報告會刪掉，送到一半的會中止。`cleat status` 會顯示目前的設定。`cleat` 指令與設定視窗永遠不會送出任何東西。

## 從原始碼建置

```sh
cd rs
cargo build --release
cargo test
bash scripts/bundle.sh    # target/bundle.noindex/Cleat.app
```

`scripts/bundle.sh` 會建出 ad-hoc 簽署、使用開發用身分 `ai.jetto.cleat.rs` 的 app，這個身分永遠不會註冊登入 agent；要在 launchd 底下測試，做法寫在 `rs/README.md`。

## 解除安裝

```sh
brew uninstall --cask --zap cleat
```

手動安裝的話：用 `launchctl bootout gui/$UID/ai.jetto.cleat` 卸載 agent，刪掉 `/Applications/Cleat.app`，再移除 `~/Library/Application Support/Cleat`、`~/Library/Logs/Cleat` 與 `~/.config/cleat`。

## 授權

MIT
