# 検証記録: dockur/windowsでの対話セッション

- 日付: 2026-10-07
- 検証項目: docs/ROADMAP.jp.mdの「対話セッション」
- ゲスト: `dockurr/windows:6.06`、`VERSION: "11"`、Windows 11 Pro build 26200、4コア、8 GB、qcow2（コミットd0d8ae0のguests/dockur-windows/compose.yaml）
- ホスト: Windows 11 Pro、Docker Desktop 28.5.1（WSL2）、AMD Ryzen 9 7950X

## 観測結果

| 確かめたこと | 方法 | 結果 |
|---|---|---|
| ゲストは自動ログオンするか | インストール直後と、スナップショットからのコールドブート後に、SSHで`query user`と`Winlogon`のレジストリを読む | `AutoAdminLogon=1`、`DefaultUserName=fixture`。セッション`console`、ID 1、Active。セッション1で`explorer`が動いている |
| SSHのコマンドはどのセッションで動くか | SSHで`(Get-Process -Id $PID).SessionId`を読む | セッション0、整合性レベルHigh、ログオン種別NETWORK |
| SSHからセッション1にプロセスを起動できるか | `-LogonType Interactive`のスケジュールタスクを`Start-ScheduledTask`で起動し、QEMUの`screendump`で画面を取る | notepadがセッション1で動き、コンソールのデスクトップに表示された |
| セッション0からそのウィンドウが見えるか | SSHで`MainWindowTitle`と`MainWindowHandle`を読む | タイトルは空、ハンドルは0 |
| UACは有効か | レジストリの`EnableLUA`を読む | dockurのインストール後は`0`。`RunLevel Limited`と`Highest`のタスクがどちらも整合性レベルHighで動いた |
| UACを有効にすると整合性レベルが分かれるか | `EnableLUA=1`にして再起動し、同じ2つのタスクで`whoami /groups`をファイルに書く | `Limited`はMedium、セッション1。`Highest`はHigh、セッション1、確認ダイアログなし。自動ログオンも続いた |
| 新規インストールで`docker compose up`からSSHが応答するまでの時間 | 60秒ごとに`ssh hostname`を試す | 27分。8.5 GBのISOのダウンロードを含む |

## 作成したスナップショット

| 名前 | 内容 |
|---|---|
| `clean` | インストール直後。sshd有効、UAC無効 |
| `uac-on` | `clean`に`EnableLUA=1`を加えたもの |

## 測れなかったこと

- スナップショットから復元してSSHが応答するまでの時間。待機ループが`query user`の終了コード1を失敗と扱ったため、表示された値（301秒）はループの上限であり、起動時間ではない。
- セッションの中からの画面キャプチャ。
