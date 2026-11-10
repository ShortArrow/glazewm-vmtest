# ゲストイメージ

Windowsゲストは、Windows 11 Proのインストールと、それに続くスナップショットの連なりでできている。ハーネスは最後のスナップショットを復元する。Hyper-Vでは`ssh-key-only`、dockur/windowsでは`no-egress`である。スナップショットはgitにないので、このページにHyper-Vとdockur/windowsそれぞれでの作り直し方を書く。

## スナップショット

各スナップショットは、1つ上のスナップショットに表の変更を加えたもの。

| スナップショット | 加えるもの |
|---|---|
| `clean` | Windows 11 Proの無人インストール。ユーザー`fixture`が自動ログオンする。`guests/dockur-windows/oem/setup-ssh.ps1`がsshdを有効にし、`guests/dockur-windows/.secrets/`の鍵を登録する。sshdはその鍵でしかログインを受け付けない |
| `uac-on` | dockur/windowsだけ。そのインストールはUACをオフにするので、`HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`の`EnableLUA=1`と、再起動を加える。Hyper-VのインストールはUACをオンのまま残すので、Hyper-Vでは`clean`の次が`vdd`になる |
| `vdd` | VirtualDrivers/Virtual-Display-Driver `25.7.23`。署名者の証明書を`LocalMachine\TrustedPublisher`に登録し、`Root\MttVDD`デバイスを作り、`pnputil`でドライバーを入れ、`C:\VirtualDisplayDriver\vdd_settings.xml`を置く |
| `vcredist` | Microsoft Visual C++ 2015〜2022 x64再頒布可能パッケージ |
| `daemon-task` | `C:\vmtest\vmtest-guest-daemon.exe 127.0.0.1:7041`を`fixture`として、対話ログオン種別、`RunLevel Limited`、トリガーなしで実行するスケジュールタスク`vmtest-guest-daemon`。TCP 7041を開けるファイアウォール規則は作らず、ハーネスはSSHのポート転送でデーモンにつなぐ。dockur/windowsでは引数なしで実行する。デーモンは引数がなければ同じアドレスで待ち受ける |
| `rdp` | Hyper-Vのみ。リモートデスクトップ接続を許可し（`fDenyTSConnections=0`）、Remote Desktopのファイアウォール規則はすべて無効のままにする。そのためTCP 3389にはゲスト自身からしか届かず、RDPクライアントはSSHのポート転送でつなぐ。SYSTEMとしてトリガーなしで実行するスケジュールタスク`vmtest-tscon`を置く。このタスクは`tscon`で`fixture`のセッションをコンソールに移す |
| `no-egress` | dockur/windowsのみで、`daemon-task`に続く。`guests/hyperv-windows/block-outbound.ps1`でファイアウォールの既定の外向き動作をBlockにし、外向きの接続をすべて遮断する。シナリオの実行中にDefender、OneDrive、Windowsの更新がゲストで始まらず、毎回同じ状態から始まるようにするためである。内向きの接続、SSHのポート転送、ループバックアドレスは影響を受けない |
| `ssh-key-only` | Hyper-Vのみ。`guests/dockur-windows/oem/ssh-key-only.ps1`で、sshdが公開鍵でしかログインを受け付けないようにする。`setup-ssh.ps1`も同じスクリプトを実行するので、今の回答ファイルのISOからインストールしたゲストでは、このスナップショットで変わるものはない |

## 資格情報

どちらの基盤も、guests/dockur-windowsにある同じものを使う。

- `.env`。`GUEST_PASSWORD=<パスワード>`で`fixture`のパスワードを書く。
- `.secrets/id_ed25519`。ハーネスが接続に使う鍵で、その公開鍵を`oem/authorized_key.pub`にコピーする。WindowsのランナーではOpenSSHが、ほかのユーザーが読める秘密鍵を使わない。`C:\`のようなドライブ直下の権限を引き継いだファイルはこれにあたる。PowerShellで`icacls .secrets\id_ed25519 /inheritance:r /grant:r "${env:USERNAME}:F"`を実行すると、持ち主だけが読めるようになる。cmdでは`${env:USERNAME}`を`%USERNAME%`にする。
- `.secrets/known_hosts`。ゲストのホスト鍵で、ハーネスが初めて接続したときに記録し、以後の接続のたびに照合する。再インストールするとゲストのホスト鍵が変わるので、このファイルからその行を消す必要がある。

## Hyper-Vで作り直す

ゲストは、ハーネス用のHyper-Vホスト上の仮想マシン`vmtest-win11`である。

1. ホストで、ホストに接続しているネットワークアダプターに`External`という外部仮想スイッチを作る。ゲストは同じネットワークからアドレスを受け取り、インターネットに出られる。
2. `guests/hyperv-windows/build-answer-iso.ps1`で回答ファイルのISOを作る。入力は`guests/hyperv-windows/answer/autounattend.xml`、`oem/setup-ssh.ps1`、`oem/authorized_key.pub`、`GUEST_PASSWORD`、インストールメディアでのProの`/IMAGE/NAME`（`Windows 11 Pro`）である。
3. 仮想マシンを作る。第2世代、ローカルのキープロテクターと仮想TPM、`MicrosoftWindows`テンプレートのセキュアブート、プロセッサ4個、固定メモリ8 GB、新しい64 GBのVHDX、`External`スイッチ、自動チェックポイントはオフ。合成ディスプレイは`Set-VMVideo -ResolutionType Single -HorizontalResolution 1280 -VerticalResolution 800`で1280×800に固定する。dockur/windowsのディスプレイも同じ大きさなので、シナリオはどちらのプラットフォームでも同じディスプレイを見て、1つのベースラインを両方で使える。Windows 11のインストールISOと回答ファイルのISOを2台のDVDドライブにつなぎ、1台目から起動する。
4. 起動し、「Press any key to boot from CD or DVD」で`root\virtualization\v2`の`Msvm_Keyboard.TypeKey`からキーを押す。最初の再起動のあとは押さず、ゲストがディスクから起動するようにする。`ssh fixture@<ゲストのアドレス>`が応答するまで待つ。アドレスは`Get-VMNetworkAdapter`で分かる。
5. `vdd`、`vcredist`、`daemon-task`、`rdp`、`ssh-key-only`の順に、手順に要るものをゲストにコピーし、SSHで次を実行する。
   - `vdd`：`guests/hyperv-windows/install-vdd.ps1 -DriverFolder <フォルダー>`。フォルダーは、リリースのアセット`VirtualDisplayDriver-x86.Driver.Only.zip`（SHA-256 `e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a`）の`VirtualDisplayDriver`フォルダー。
   - `vcredist`：`guests/hyperv-windows/install-vcredist.ps1`。
   - `daemon-task`：`guests/hyperv-windows/install-daemon-task.ps1 -User fixture`。
   - `rdp`：`guests/hyperv-windows/install-rdp.ps1 -User fixture`。
   - `ssh-key-only`：`guests/dockur-windows/oem/ssh-key-only.ps1`。
6. `clean`のあとと各手順のあとに、コピーしたものを消し、ゲスト内からシャットダウンし（`shutdown /s /t 0`）、仮想マシンが止まってから`Checkpoint-VM -SnapshotName <名前>`で標準チェックポイントを取る。

## dockur/windowsで作り直す

ゲストはguests/dockur-windows/compose.yamlで決まり、スナップショットはDockerのボリューム`glazewm-vmtest-win11-storage`内のqcow2ディスクにある。Dockerホストには`/dev/kvm`が要る。

### Hyper-V上のDockerホスト

Dockerホストには、ハーネスのHyper-VホストにあるLinuxの仮想マシン`vmtest-kvm`を使える。ネストした仮想化で`/dev/kvm`を持たせる。

1. https://cloud-images.ubuntu.com/releases/noble/release/ から`ubuntu-24.04-server-cloudimg-amd64.img`をダウンロードし、同じ場所の`SHA256SUMS`と照合する。
2. `dockurr/windows`イメージの`qemu-img resize`で160 GBに広げ、`qemu-img convert -O vhdx -o subformat=dynamic`で容量可変のVHDXに変換する。
3. ランナーがホストにログオンするときの公開鍵から、`guests/docker-host/build-seed-iso.sh <公開鍵のファイル> seed.iso`でシードISOを作る。ホストは初回起動時に、その鍵でログオンするパスワードなしのユーザー`vmtest`を作り、パスワードでのログインを無効にし、Dockerをインストールする。
4. 仮想マシンを作る。第2世代、`MicrosoftUEFICertificateAuthority`テンプレートのセキュアブート、`Set-VMProcessor -ExposeVirtualizationExtensions $true`を付けた8プロセッサ、16 GBの静的メモリ、作ったVHDX、DVDドライブにシードISO、`External`スイッチ、自動チェックポイントなし。
5. 起動し、`ssh vmtest@<ホストのアドレス> cloud-init status --wait`が`done`を返すまで待つ。イメージがHyper-Vのデータ交換デーモンを動かさないので、`Get-VMNetworkAdapter`にはアドレスが出ない。アドレスはネットワークのDHCPのリースかARPテーブルで分かる。
6. guests/dockur-windowsのcompose.yaml、`.env`、`oem/`を、ホストの`dockur-windows`という名前のディレクトリにコピーする。Docker Composeはそのディレクトリ名をプロジェクト名にし、ハーネスのcomposeコマンドはその名前でコンテナを探す。

`VMTEST_DOCKER_HOST=vmtest@<ホストのアドレス>`を設定すると、ハーネスはdockerのコマンドをそのホストで実行し、ゲストのSSHのポートにもそのホストを経由してつなぐ。

### ゲスト

1. Dockerホストのguests/dockur-windowsで`docker compose up -d`を実行し、そのホストで`ssh -p 2222 fixture@127.0.0.1`が、ランナーからなら`ssh -J vmtest@<ホストのアドレス> -p 2222 fixture@127.0.0.1`が応答するまで待つ。
2. 表のうちdockur/windowsに当てはまる各行について、SSHで変更を加え、ゲスト内からシャットダウンし（`shutdown /s /t 0`）、`dockurr/windows`イメージの`qemu-img snapshot -c <名前> /storage/data.qcow2`でスナップショットを作る。手順はdockur-windowsスキルのとおり。`vdd`、`vcredist`、`no-egress`の変更は、guests/hyperv-windowsのスクリプトで加えられる。
