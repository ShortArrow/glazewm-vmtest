# ゲストイメージ

Windowsゲストは、Windows 11 Proのインストールと、それに続くスナップショットの連なりでできている。ハーネスは最後のスナップショットを復元する。Hyper-Vでは`rdp`、dockur/windowsでは`daemon-task`である。スナップショットはgitにないので、このページにHyper-Vとdockur/windowsそれぞれでの作り直し方を書く。

## スナップショット

各スナップショットは、1つ上のスナップショットに表の変更を加えたもの。

| スナップショット | 加えるもの |
|---|---|
| `clean` | Windows 11 Proの無人インストール。ユーザー`fixture`が自動ログオンする。`guests/dockur-windows/oem/setup-ssh.ps1`がsshdを有効にし、`guests/dockur-windows/.secrets/`の鍵を登録する |
| `uac-on` | dockur/windowsだけ。そのインストールはUACをオフにするので、`HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`の`EnableLUA=1`と、再起動を加える。Hyper-VのインストールはUACをオンのまま残すので、Hyper-Vでは`clean`の次が`vdd`になる |
| `vdd` | VirtualDrivers/Virtual-Display-Driver `25.7.23`。署名者の証明書を`LocalMachine\TrustedPublisher`に登録し、`Root\MttVDD`デバイスを作り、`pnputil`でドライバーを入れ、`C:\VirtualDisplayDriver\vdd_settings.xml`を置く |
| `vcredist` | Microsoft Visual C++ 2015〜2022 x64再頒布可能パッケージ |
| `daemon-task` | `C:\vmtest\vmtest-guest-daemon.exe 127.0.0.1:7041`を`fixture`として、対話ログオン種別、`RunLevel Limited`、トリガーなしで実行するスケジュールタスク`vmtest-guest-daemon`。TCP 7041を開けるファイアウォール規則は作らず、ハーネスはSSHのポート転送でデーモンにつなぐ。dockur/windowsでは引数なしで実行して`0.0.0.0:7041`で待ち受け、ファイアウォール規則`vmtest-guest-daemon`でTCP 7041の受信を許す。コンテナがそのポートをホストのループバックにしか公開しないので、ネットワークからは届かない |
| `rdp` | Hyper-Vのみ。リモートデスクトップ接続を許可し（`fDenyTSConnections=0`）、Remote Desktopのファイアウォール規則はすべて無効のままにする。そのためTCP 3389にはゲスト自身からしか届かず、RDPクライアントはSSHのポート転送でつなぐ。SYSTEMとしてトリガーなしで実行するスケジュールタスク`vmtest-tscon`を置く。このタスクは`tscon`で`fixture`のセッションをコンソールに移す |

## 資格情報

どちらの基盤も、guests/dockur-windowsにある同じものを使う。

- `.env`。`GUEST_PASSWORD=<パスワード>`で`fixture`のパスワードを書く。
- `.secrets/id_ed25519`。ハーネスが接続に使う鍵で、その公開鍵を`oem/authorized_key.pub`にコピーする。
- `.secrets/known_hosts`。ゲストのホスト鍵で、ハーネスが初めて接続したときに記録し、以後の接続のたびに照合する。再インストールするとゲストのホスト鍵が変わるので、このファイルからその行を消す必要がある。

## Hyper-Vで作り直す

ゲストは、ハーネス用のHyper-Vホスト上の仮想マシン`vmtest-win11`である。

1. ホストで、ホストに接続しているネットワークアダプターに`External`という外部仮想スイッチを作る。ゲストは同じネットワークからアドレスを受け取り、インターネットに出られる。
2. `guests/hyperv-windows/build-answer-iso.ps1`で回答ファイルのISOを作る。入力は`guests/hyperv-windows/answer/autounattend.xml`、`oem/setup-ssh.ps1`、`oem/authorized_key.pub`、`GUEST_PASSWORD`、インストールメディアでのProの`/IMAGE/NAME`（`Windows 11 Pro`）である。
3. 仮想マシンを作る。第2世代、ローカルのキープロテクターと仮想TPM、`MicrosoftWindows`テンプレートのセキュアブート、プロセッサ4個、固定メモリ8 GB、新しい64 GBのVHDX、`External`スイッチ、自動チェックポイントはオフ。合成ディスプレイは`Set-VMVideo -ResolutionType Single -HorizontalResolution 1280 -VerticalResolution 800`で1280×800に固定する。dockur/windowsのディスプレイも同じ大きさなので、シナリオはどちらのプラットフォームでも同じディスプレイを見て、1つのベースラインを両方で使える。Windows 11のインストールISOと回答ファイルのISOを2台のDVDドライブにつなぎ、1台目から起動する。
4. 起動し、「Press any key to boot from CD or DVD」で`root\virtualization\v2`の`Msvm_Keyboard.TypeKey`からキーを押す。最初の再起動のあとは押さず、ゲストがディスクから起動するようにする。`ssh fixture@<ゲストのアドレス>`が応答するまで待つ。アドレスは`Get-VMNetworkAdapter`で分かる。
5. `vdd`、`vcredist`、`daemon-task`、`rdp`の順に、手順に要るものをゲストにコピーし、SSHで次を実行する。
   - `vdd`：`guests/hyperv-windows/install-vdd.ps1 -DriverFolder <フォルダー>`。フォルダーは、リリースのアセット`VirtualDisplayDriver-x86.Driver.Only.zip`（SHA-256 `e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a`）の`VirtualDisplayDriver`フォルダー。
   - `vcredist`：`guests/hyperv-windows/install-vcredist.ps1`。
   - `daemon-task`：`guests/hyperv-windows/install-daemon-task.ps1 -User fixture`。
   - `rdp`：`guests/hyperv-windows/install-rdp.ps1 -User fixture`。
6. `clean`のあとと各手順のあとに、コピーしたものを消し、ゲスト内からシャットダウンし（`shutdown /s /t 0`）、仮想マシンが止まってから`Checkpoint-VM -SnapshotName <名前>`で標準チェックポイントを取る。

## dockur/windowsで作り直す

ゲストはguests/dockur-windows/compose.yamlで決まり、スナップショットはDockerのボリューム`glazewm-vmtest-win11-storage`内のqcow2ディスクにある。

1. guests/dockur-windowsで`docker compose up -d`を実行し、`ssh -p 2222 fixture@127.0.0.1`が応答するまで待つ。
2. 表の各行について、SSHで変更を加え、ゲスト内からシャットダウンし（`shutdown /s /t 0`）、`dockurr/windows`イメージの`qemu-img snapshot -c <名前> /storage/data.qcow2`でスナップショットを作る。手順はdockur-windowsスキルのとおり。`vdd`と`vcredist`の変更は、guests/hyperv-windowsのスクリプトで加えられる。
