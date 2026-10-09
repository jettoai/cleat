//! Every word the settings window and the menu bar show, in Traditional Chinese and English.
//! `{}` marks where `text::fill` puts a value; both languages carry the same number of them, in
//! the same order.

use super::lang::Lang;

macro_rules! words {
    ($($key:ident: $zh:literal, $en:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum W { $($key),* }
        impl W {
            pub const ALL: &'static [W] = &[$(W::$key),*];
            pub fn zh(self) -> &'static str { match self { $(W::$key => $zh),* } }
            pub fn en(self) -> &'static str { match self { $(W::$key => $en),* } }
            pub fn get(self, l: Lang) -> &'static str {
                match l { Lang::ZhHant => self.zh(), Lang::En => self.en() }
            }
        }
    };
}

words! {
    // Menu bar (app/menubar.rs)
    Loading: "讀取中…", "Reading…";
    MenuOutput: "輸出：{}", "Output: {}";
    MenuInput: "輸入：{}", "Input: {}";
    OpenSettings: "開啟設定…", "Settings…";
    About: "關於 Cleat", "About Cleat";
    Quit: "結束 Cleat", "Quit Cleat";

    // The window's own menu and title
    QuitSettings: "結束 Cleat 設定", "Quit Cleat Settings";
    FileMenu: "檔案", "File";
    CloseWindow: "關閉視窗", "Close Window";
    WindowTitle: "Cleat 設定", "Cleat Settings";

    // Errors
    WriteNotObject: "設定檔不是 JSON 物件，畫面不會覆寫它。", "The config file is not a JSON object, so the window will not overwrite it.";
    WriteChanged: "設定檔在畫面開著時被別處改過，請重新載入。", "The config file changed while the window was open. Reload it to go on.";
    WriteInvalid: "寫出的設定不合法：{}", "The settings to write are not valid: {}";
    WriteIo: "寫入失敗：{}", "Could not write the config file: {}";
    LaunchRegisterFailed: "開機自動啟動沒有設定成功：{}", "Could not turn on start at login: {}";
    Unreadable: "設定檔無法讀取：{}。修好檔案後按重新載入。", "Cleat cannot read the config file: {}. Fix the file, then click Reload.";
    Reload: "重新載入", "Reload";

    // Pages
    PageOutput: "輸出", "Output";
    PageInput: "輸入", "Input";
    PageHeadphones: "耳機", "Headphones";
    PageGeneral: "一般", "General";
    SubOutput: "Cleat 讓聲音一直從你排第一的裝置出來", "Cleat keeps the sound on the first device in your list";
    SubInput: "Cleat 讓你排第一的麥克風一直是預設輸入，音量停在你設定的值", "Cleat keeps your first microphone the default input, at the level you set";
    SubHeadphones: "藍牙耳機被手機或 iPad 拿走時，Cleat 把它要回來", "Cleat gets your Bluetooth headphones back from a phone or iPad";
    SubGeneral: "Cleat 什麼時候執行", "When Cleat runs";

    // Device lists
    FooterInput: "清單中第一個已連線的麥克風會成為預設輸入。勾「不使用」的裝置，Cleat 永遠不會切過去。", "The first connected microphone on the list becomes the default input. Cleat never switches to an excluded device.";
    FooterOutput: "有聲音要播時，Cleat 會切到清單中第一個已連線的裝置。勾「不使用」的裝置，Cleat 永遠不會切過去。", "When there is sound to play, Cleat switches to the first connected device on the list. Cleat never switches to an excluded device.";
    NeverUse: "不使用", "Exclude";
    BlockHelp: "勾了之後，Cleat 永遠不會切到這個裝置", "When ticked, Cleat never switches to this device";
    NotSetUpOutput: "尚未設定，Cleat 不會切換輸出", "Not set up, so Cleat does not switch the output";
    NotSetUpInput: "尚未設定，Cleat 不會切換輸入", "Not set up, so Cleat does not switch the input";
    RemoveFromOrder: "移出順序", "Remove";
    MoveUp: "上移", "Move Up";
    MoveDown: "下移", "Move Down";
    RemoveFromPriority: "移出優先順序", "Remove from Priority List";
    PriorityOutput: "輸出優先順序", "Output priority";
    PriorityInput: "輸入優先順序", "Input priority";
    AddToOrder: "加入順序", "Add to order";
    OthersOutput: "其他輸出裝置", "Other output devices";
    OthersInput: "其他輸入裝置", "Other input devices";
    NotConnected: "未連線", "Offline";
    StuckRowNote: "暫時還在用這台", "Still in use for now";
    TagStuck: "已勾不使用，但沒有其他裝置可切", "Nowhere else to go";
    TagPaused: "已勾不使用，被切回來、暫停中", "Paused, keeps coming back";
    Excluded: "已排除", "Excluded";
    InUse: "使用中", "In use";
    StuckOutput: "沒有其他可用的輸出裝置，暫時還在用 {}。取消其他裝置的「不使用」並加入順序，或接上清單內的裝置。", "No other output device can be used, so it stays on {} for now. Untick Exclude on another device and add it to the order, or connect a device on the list.";
    StuckInput: "沒有其他可用的輸入裝置，暫時還在用 {}。取消其他裝置的「不使用」並加入順序，或接上清單內的裝置。", "No other input device can be used, so it stays on {} for now. Untick Exclude on another device and add it to the order, or connect a device on the list.";
    PausedOutput: "其他程式或裝置一直把輸出切回 {}，Cleat 先暫停把它換掉，直到裝置增減或設定改變才再試。想馬上再試，拔插一個裝置即可。", "Something keeps switching the output back to {}, so Cleat has paused moving it off. It tries again once a device is added or removed or the settings change. To try now, unplug and replug any device.";
    PausedInput: "其他程式或裝置一直把輸入切回 {}，Cleat 先暫停把它換掉，直到裝置增減、設定改變，或麥克風有聲無聲翻轉才再試。想馬上再試，拔插一個裝置即可。", "Something keeps switching the input back to {}, so Cleat has paused moving it off. It tries again once a device is added or removed, the settings change, or a microphone's signal comes or goes. To try now, unplug and replug any device.";

    // Levels
    OutputVolume: "輸出音量", "Output volume";
    HoldAgainst: "被其他程式改掉時拉回", "Undo changes made by these programs";
    AddApp: "加入程式…", "Add app…";
    PanelAdd: "加入", "Add";
    NoUndoYet: "還沒有拉回紀錄", "No undo yet";
    RevertLine: "最近一次：{} 拉回 {} 改的音量 {}% → {}%", "Last undo {}: {} set {}%, put back to {}%";
    RevertLegacy: "最近一次：{}", "Last undo at {}";
    Hold: "固定", "Hold";
    Balance: "左右平衡", "Balance";
    BalanceLeft: "左 L", "L";
    BalanceRight: "R 右", "R";
    Centre: "置中", "Centre";
    LeftPct: "偏左 {}%", "Left {}%";
    RightPct: "偏右 {}%", "Right {}%";
    NoBalance: "現在：{}不支援左右平衡，Cleat 不會動它", "Now: {} has no balance control, left alone";
    NowNoneOutput: "現在：沒有預設輸出裝置", "Now: no default output device";
    NowNoneInput: "現在：沒有預設輸入裝置", "Now: no default input device";
    NowNoReading: "現在：{} 讀不到這個值", "Now: {} gives no reading";
    NowReading: "現在：{} {}", "Now: {} {}";
    OutputLevelsHeader: "輸出音量與平衡", "Output volume and balance";
    OutputLevelsFooter: "輸出音量：名單上的程式改了音量，Cleat 會拉回原本的值；你自己調的不會被拉回。左右平衡：打開「固定」後，被別的 app 或藍牙重連改掉時 Cleat 會改回來。", "Output volume: when a program on the list changes it, Cleat puts it back; your own changes stay. Balance: with Hold on, Cleat puts it back when another app or a Bluetooth reconnect changes it.";
    DefaultMicLevel: "所有麥克風的預設音量", "Default level for every microphone";
    AddMicrophone: "新增麥克風", "Add microphone";
    InputVolume: "輸入音量", "Input volume";
    VolumesFooter: "單獨設定的麥克風優先於預設音量。", "A microphone with its own level overrides the default.";

    // Vitals band
    Running: "Cleat 執行中", "Cleat is running";
    NotRunning: "Cleat 未執行", "Cleat is not running";
    VitalsUnreadable: "讀不到 Cleat 的用量", "Cannot read Cleat's usage";
    Measuring: "量測中", "Reading";
    CpuNote: "一顆核心的百分比", "Percent of one core";
    CpuWindow: "最近 {} 秒平均", "{}-second average";
    CpuHelp: "Cleat 常駐程式占一顆核心的百分比，跟活動監視器同一個算法；取最近 60 秒的平均，剛打開視窗時是打開以來的平均", "Share of one core the Cleat daemon uses, counted as Activity Monitor does. Averaged over the last 60 seconds, or since the window opened if that is sooner";
    Memory: "記憶體", "Memory";
    PhysicalMemory: "實體記憶體", "Physical memory";
    MemoryHelp: "與活動監視器「記憶體」欄同一個值", "The same figure as Activity Monitor's Memory column";
    ReclaimSpeed: "拉回速度", "Reclaim speed";
    ReactionNone: "還沒有拉回紀錄", "Nothing put back yet";
    ReactionMedian: "最近 {} 次的中位數", "Median of the last {}";
    ReactionNotRunning: "Cleat 沒在執行", "Cleat is not running";
    ReactionOldDaemon: "這個版本的 Cleat 還不會量拉回速度", "This version of Cleat does not measure reclaim speed";
    ReactionHelp: "其他程式或系統改掉你的設定後，Cleat 改回來要多久。最近一次共 {}，其中 Cleat 自己處理 {}，其餘是刻意等裝置穩定", "How long Cleat takes to put a setting back after a program or the system changed it. Last time: {} in all, {} of it Cleat's own work, the rest a deliberate wait for the device to settle";

    // Headphones
    TakeOver: "藍牙耳機連上時自動切過去", "Switch to Bluetooth headphones when they connect";
    Reclaim: "耳機被其他裝置拿走時要回來", "Ask for headphones back from other devices";
    NoPairedHeadsets: "沒有找到配對過的藍牙耳機", "No paired Bluetooth headphones found";
    OtherBluetooth: "其他藍牙裝置（{}）", "Other Bluetooth devices ({})";
    OthersNote: "系統沒說是什麼的藍牙裝置。喇叭或耳機不在上面時，到這裡勾；手機、電腦不用勾。", "Bluetooth devices the system does not identify. Tick a speaker or headphones here if they are missing above; phones and computers need no tick.";
    HeadsetBlockedNote: "輸出設為不使用，不會拉回", "Excluded from output, not asked back";
    HeadsetBlockedOffNote: "輸出設為不使用", "Excluded from output";
    AllHeadsetsBlocked: "勾選的耳機都設為不使用，不會有動作", "Every ticked headset is excluded from output";
    NoHeadsets: "還沒有配對過的耳機，不會有動作", "No headphones paired yet, so nothing happens";
    AllHeadsetsBlockedHint: "所有耳機都設為不使用，不會有動作。到「輸出」頁取消「不使用」即可。", "Every headset is excluded on the Output page. Untick Exclude there.";
    NoneTicked: "請至少勾選一副耳機，否則不會有動作", "Tick at least one headset, or nothing happens";

    // General
    LaunchAtLogin: "開機自動啟動", "Start at login";
    StartupSection: "啟動", "Startup";
    LaunchAtLoginNote: "開著時，開機登入後就會啟動 Cleat，當掉也會自動重開。關掉後 Cleat 會結束，之後要用時從「應用程式」資料夾打開。", "While on, Cleat starts when you log in and restarts if it crashes. Turning it off quits Cleat; to use it again, open it from the Applications folder.";
}
