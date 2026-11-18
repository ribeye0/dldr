# dldr 命令列使用方式

程式名稱是 **dldr**（DLsite Downloader）。從 Release 下載 CLI 壓縮檔，解壓後
執行 `./dldr`，或按照 [Linux 安裝說明](linux-install.md) 安裝到 PATH。
CLI 與 GUI 共用資料目錄與帳號，不必搬移作品。
預設設定目錄為 Linux 的 `~/.config/dldr/`（遵循 `XDG_CONFIG_HOME`），
Windows 的 `%APPDATA%\dldr\`。舊設定不會自動搬移或讀取。

## 第一次使用與登入

先關閉 GUI，再執行：

```sh
dldr login                        # 輸入登入 ID／email、密碼，必要時輸入驗證碼
dldr RJ01760323                    # 下載一部已購作品
```

登入 ID、密碼與驗證碼都以終端隱藏輸入，不會寫進命令歷史。登入成功後才保存
帳號、密碼與工作階段，並自動同步購買清單；不會自動下載作品。錯誤的密碼、
驗證失敗或取消登入不覆寫既有帳號。首次使用會建立資料目錄與資料庫，未設定的
作品庫預設為系統 Downloads 下的 `DLDR`，暫存預設為應用程式資料目錄下的
`downloads`；現有路徑設定保持原值。下載時才建立作品目錄。

```sh
dldr login --name '個人帳號'        # 為新帳號命名
dldr login -a '個人帳號'            # 更新既有帳號；登入 ID 可按 Enter 沿用
dldr login --no-sync               # 只登入；稍後用 dldr sync 更新購買清單
dldr accounts                     # 查看帳號 ID、名稱及是否啟用，不輸出登入 ID
dldr logout                       # 僅有一個帳號時直接登出
dldr logout -a '個人帳號'           # 多帳號時指定 ID 或唯一完整名稱
```

再次輸入相同登入 ID 會重用現有帳號；同一 ID 有多個記錄時須明確指定 `-a`。
登出會清除該帳號本機保存的密碼與工作階段，並停用帳號；購買紀錄、播放清單與
已下載檔案仍保留。重新登入後啟用。登出不會撤銷其他裝置上的 DLsite 工作階段。
密碼與工作階段使用和 GUI 相同的本機憑證檔；Unix 目錄／檔案權限為 0700／0600，
目前不是加密保管庫。CLI 不提供密碼／驗證碼旗標、環境變數或管線輸入。
**Ctrl+C** 或 **Esc** 取消互動；自動化可使用已保存帳號，但遇到二階段驗證時
需要互動終端。`--json` 只改變結果格式，登入仍須終端輸入。

## 下載與續傳

```sh
dldr RJ01760323                             # 已完成作品自動略過
dldr RJ01760323 RJ01729125 -j 2              # 同時下載兩部
dldr rj01760323 --limit-rate 5M              # 忽略 ID 大小寫，整體限速 5 MiB/s
dldr RJ01760323 -o '/media/disk/音聲'         # 本次作品庫路徑
dldr RJ01760323 --dry-run                    # 唯讀預覽容量，不連線、不下載
dldr RJ01760323 -a '個人帳號'                # 指定擁有作品的帳號
dldr status                                 # 儲存的佇列狀態
dldr resume                                 # 續傳暫停／失敗的作品
dldr resume RJ01760323 -j 1 --limit-rate 2M
```

`-j/--jobs` 範圍是 1–4。`--limit-rate` 的 K/M/G 使用 1024 進位，支援 `1.5M`、
`5MiB/s`，`0` 代表不限速。預設自動解壓縮，成功後刪除壓縮檔；可在介面下載設定改為保留壓縮檔。`--extract` 強制自動解壓縮並在成功後刪除壓縮檔，`--keep-archives` 強制保留壓縮檔；`--temp DIR`
指定暫存目錄。未指定時沿用 GUI 的下載設定；命令選項只影響當次，不修改預設值。

下載顯示階段、進度與速率。按 Ctrl+C 會停止開始的工作，保留部分檔案，並暫停
未完成佇列；之後執行 `dldr resume`。續傳沿用原本的帳號、路徑與解壓設定。
`status` 是持久化狀態，不是 GUI 即時速率監看。

作品必須已同步到快取並屬於啟用帳號。剛購買的新作品請先 `dldr sync`。
未輸入作品 ID 時只顯示說明；只有旗標卻沒有選擇作品會報錯，絕不默認下載全部。
既有的 `dldr download ID...`／`dldr dl ID...` 仍可用。

## 搜尋與批次下載

```sh
dldr search NTR
dldr search 青梅竹馬
dldr search '$va:小花衣こっこ$'
dldr search '$circle:バイコーンの森$ $tag:女性主导$'
dldr list --downloaded                     # 只列出已下載作品
dldr show RJ01760323                       # 名稱、社團、聲優、分類、容量與路徑
dldr download --search NTR --dry-run        # 先預覽選中作品與快取容量
dldr download --search NTR                  # 確認數量後才下載
dldr download --search NTR --yes            # 腳本明確接受搜尋結果
```

搜尋只讀本機快取，不連線。`$…$` 使用單引號，避免 shell 展開變數。
和 GUI 一樣，多條件以 AND 組合。`-n 50` 改每頁筆數，`-p 2` 顯示第二頁。
`list`、`ls`、`s` 都是 `search` 的別名。分類搜尋接受繁中、簡中、日文與英文；
顯示值沿用已儲存的介面語言，命令、說明與 JSON 欄位維持英文。

搜尋下載需要確認；沒有互動終端或搭配 `--json` 時必須加 `--yes`。
RJ 列表與 `--search` 不可混用。查不到、格式錯誤或不屬於指定帳號時會明確報錯。

## 其他命令與全域選項

```sh
dldr sync                        # 同步所有啟用帳號
dldr sync -a '個人帳號'
dldr config                      # 查看下載路徑、並行數、限速與顯示語言
dldr open                        # 開啟作品庫資料夾
dldr open RJ01760323              # 開啟已下載作品資料夾
dldr open RJ01760323 --print      # 只印出路徑
dldr gui                         # 開啟桌面版
dldr --help
dldr login --help
dldr --version
```

| 選項 | 用途 |
| --- | --- |
| `--json` | stdout 輸出單一 JSON 文件，方便腳本處理 |
| `-q, --quiet` | 隱藏進度和提示訊息，保留結果與錯誤 |
| `--data-dir DIR` | 使用另一個資料目錄；`login` 可建立，查詢不會自動建立 |
| `-h, --help` | 顯示說明 |
| `-V, --version` | 顯示版本 |

下載選項放在 RJ 號前後都可；子命令的專用選項放在子命令之後。
全域選項可放在子命令前後。`--data-dir` 不適用於 `gui`。

**搜尋、詳情、狀態與 dry-run 可在 GUI 開啟時使用；登入、登出、同步、下載、
續傳之前須關閉 GUI。** 兩者使用同一把程序鎖，避免憑證互相覆寫或恢復下載佇列
干擾另一個程序。目前沒有背景服務或控制執行中 GUI 的通訊介面。

JSON 結果使用 stdout；進度、互動提示與錯誤使用 stderr。結束碼為 0 成功、
1 執行或部分作品失敗、2 參數錯誤／缺少搜尋下載確認、130 使用者中斷。
拒絕下載確認正常結束且不新增佇列。
