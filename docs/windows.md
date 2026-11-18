# DLDR Windows 測試版

目標為 **Windows 11 x64**。這是尚待原生驗收、未簽署的測試建置。

## 啟動

解壓縮 `DLDR_VERSION_windows_x64.zip`，開啟其中 `DLDR/DLDR.Desktop.exe`。
請保留同資料夾的 `libmpv-2.dll` 及 `third-party`；只複製 EXE 會使本機播放器
無法載入。程式資料保存在 Windows 使用者的應用程式資料目錄，並非解壓縮目錄。
資料庫、帳號憑證、播放紀錄、日誌及瀏覽器資料統一存於 `%APPDATA%\dldr\`。
`%LOCALAPPDATA%\DLDR\` 是安裝目錄，並非設定目錄。
Linux 的帳號與下載資料不會自動搬入 Windows。
桌面程式與命令列使用不同檔名，避免 Windows 不區分大小寫造成覆蓋。

也可以執行 `DLDR_VERSION_x64-setup.exe` 安裝到目前使用者的目錄，建立選單捷徑。
安裝程式會在缺少 Microsoft Edge WebView2 Runtime 時下載安裝。免安裝 ZIP
需要系統已有 WebView2；可從 [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/)
安裝 Evergreen Runtime。Windows 11 通常已提供。

## 首次使用

第一次啟動會依序引導設定 DLsite 帳號、登入並同步作品、確認下載目錄。
預設在系統「下載」目錄建立 `dldr` 資料夾，完成作品存於此，暫存檔分開
存於 `.staging` 子目錄；完成設定前會確認目錄可寫入。引導不會下載作品。
若中途關閉，下次會從尚未完成的步驟繼續，已有完整設定的資料不會被覆蓋。

## CLI

在解壓縮資料夾開啟 PowerShell：

```powershell
.\dldr.exe --version
.\dldr.exe login
.\dldr.exe RJ01760323 --dry-run
.\dldr.exe gui
```

登入／同步／下載等會修改資料的 CLI 操作前，先關閉 GUI。
命令與驗證碼輸入方式見 [CLI 說明](https://github.com/ribeye0/dldr/blob/main/docs/dldr.md)。
帳號密碼與工作階段目前保存在本機 JSON 憑證檔，尚未改用 Windows Credential Manager。

## Windows 11 驗收

先確認視窗、圖示、四語介面、搜尋補全與資料夾設定可用，再測試：

- 使用自己選定的已購作品登入／同步、下載、暫停、重啟續傳、解壓與開啟資料夾。
- 本機 WAV、MP3、FLAC、M4A、Ogg 音軌、音訊輸出與 VTT 台本。
- DLsite Play 官方登入、播放／字幕，以及作品頁的下載按鈕。
- 中文／日文與空白路徑、安裝／移除、更新後保留資料。

Wine 可用於部分 CLI、DLL 與檔案操作檢查；其 WebView2 相容性不能替代 Windows 11
桌面驗收。測試不應共用 Linux 實帳 profile。公開發佈前須完成原生驗收，並依
[發佈流程](https://github.com/ribeye0/dldr/blob/main/docs/releasing.md) 整理第三方通知與
精確對應的完整 runtime 原始碼；測試包中的應用程式 source archive 不包含所有
libmpv 靜態相依套件的原始碼。
