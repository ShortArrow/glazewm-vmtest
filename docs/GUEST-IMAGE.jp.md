# ゲストイメージ

dockur/windows用のWindowsゲストは、guests/dockur-windows/compose.yamlと、そのqcow2ディスク内のスナップショットで決まる。スナップショットはgitではなくDockerのボリューム`glazewm-vmtest-win11-storage`にあるので、このページに作り直し方を書く。

## スナップショット

各スナップショットは、1つ上のスナップショットに表の変更を加えたもの。ハーネスは`daemon-task`を復元する。

| スナップショット | 加えるもの |
|---|---|
| `clean` | Windows 11 Proの無人インストール。ユーザー`fixture`が自動ログオンする。`oem/setup-ssh.ps1`がsshdを有効にし、`.secrets/`の鍵を登録する |
| `uac-on` | `HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`の`EnableLUA=1`と、再起動 |
| `vdd` | VirtualDrivers/Virtual-Display-Driver `25.7.23`。署名者の証明書を`LocalMachine\TrustedPublisher`に登録し、`Root\MttVDD`デバイスを作り、`pnputil`でドライバーを入れ、`C:\VirtualDisplayDriver\vdd_settings.xml`を置く |
| `vcredist` | Microsoft Visual C++ 2015〜2022 x64再頒布可能パッケージ |
| `daemon-task` | TCP 7041の受信を許すファイアウォール規則`vmtest-guest-daemon`。`C:\vmtest\vmtest-guest-daemon.exe`を`fixture`として、対話ログオン種別、`RunLevel Limited`、トリガーなしで実行するスケジュールタスク`vmtest-guest-daemon` |

## 作り直し方

1. `guests/dockur-windows/.env`に`GUEST_PASSWORD=<パスワード>`を書き、`.secrets/id_ed25519`を生成して、その公開鍵を`oem/authorized_key.pub`にコピーする。
2. guests/dockur-windowsで`docker compose up -d`を実行し、`ssh -p 2222 fixture@127.0.0.1`が応答するまで待つ。
3. 上の表の各行について、SSHで変更を加え、ゲスト内からシャットダウンし（`shutdown /s /t 0`）、`dockurr/windows`イメージの`qemu-img snapshot -c <名前> /storage/data.qcow2`でスナップショットを作る。手順はdockur-windowsスキルのとおり。

各手順で使ったコマンドはdocs/spikes/にある。
