# DLDR Windows

支援 **Windows 11 x64**。安裝檔尚未使用程式碼簽署憑證，Windows 可能顯示 SmartScreen 提示。

## 啟動

解壓縮 `DLDR_VERSION_windows_x64.zip`，開啟其中 `DLDR/DLDR.Desktop.exe`。
請保留整個 `DLDR` 資料夾，包括 mpv、libarchive 的 DLL 與 `third-party`；
只複製 EXE 會使播放器或壓縮檔讀取無法載入。程式資料保存在 Windows
使用者的應用程式資料目錄，並非解壓縮目錄。免安裝版與安裝版共用此設定目錄，
測試時請分別啟動，不要同時執行。
資料庫、帳號憑證、播放紀錄、日誌及瀏覽器資料統一存於 `%APPDATA%\dldr\`。
`%LOCALAPPDATA%\DLDR\` 是安裝目錄，並非設定目錄。
Linux 的帳號與下載資料不會自動搬入 Windows。
桌面程式與命令列使用不同檔名，避免 Windows 不區分大小寫造成覆蓋。

也可以執行 `DLDR_VERSION_x64-setup.exe` 安裝到目前使用者的目錄，建立選單捷徑。
安裝程式會在缺少 Microsoft Edge WebView2 Runtime 時下載安裝。免安裝 ZIP
需要系統已有 WebView2；可從 [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/)
安裝 Evergreen Runtime。Windows 11 通常已提供。

## 更新

到「設定 → 關於與更新 → 檢查更新」，確認版本後按「下載更新」。
下載可以取消，也可以關閉視窗繼續使用 DLDR；完成後再按「安裝並重新啟動」。
「安裝本機套件」可選擇與目前形式相同的免安裝 ZIP 或安裝 EXE 進行升降級。
未簽署的自行建置版需要明確勾選信任，降級前請先匯出作品庫備份。
安裝版會執行新版安裝程式；免安裝版會驗證 ZIP，等待舊程式退出後替換整組
檔案並重新啟動，維持免安裝形式。替換或啟動失敗時會還原舊檔案；未能還原的
備份會保留於應用程式目錄的 `.dldr-update-backup-*` 資料夾。
設定與下載作品不會刪除。更新前請先完成或暫停下載，並等待同步結束。
Windows 的更新包簽章用於 DLDR 驗證，與 SmartScreen 的程式碼簽署憑證不同。
沒有更新功能的舊版需先手動升級一次。

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
桌面驗收。測試不應共用 Linux 實帳 profile。

同一 Release 提供應用程式與 Windows runtime 原始碼壓縮檔；後者包含
libmpv、FFmpeg、libarchive 及相依 DLL 的精確對應來源與建置配方。
下載後可使用 `SHA256SUMS.txt` 檢查檔案完整性。
