# CI

ハーネスは2つのGitHub Actionsのワークフローで確かめる。

| ワークフロー | 起動する条件 | 実行する場所 | 確かめること |
|---|---|---|---|
| `.github/workflows/ci.yml` | `main`へのプッシュと、すべてのプルリクエスト | `windows-latest` | `cargo fmt --check`、`cargo clippy -D warnings`、ゲストの要らないテスト |
| `.github/workflows/live.yml` | `main`へのプッシュと、手動 | ビルドは`windows-latest`、その後セルフホストのランナー`vmtest-kvm` | dockur/windowsのゲストに対するすべてのliveシナリオを、下のglazewmごとに1回ずつ |

## liveのワークフロー

Windowsのジョブは、ゲストデーモン、テストウィンドウのアプリケーション、負荷生成プログラムを1回ビルドし、glazewmとzebarを次のコミットの組ごとに、ワークフローが固定したnightlyのツールチェーンでビルドする。どの組もglazewmをユーザーとして起動して実行し、`all-fixes`は、`sudo glazewm`と同じくglazewmを昇格して起動してもう1回実行する。

| 組 | glazewm | zebar | 期待する結果 |
|---|---|---|---|
| `upstream` | glzr-io/glazewmの`5709ad0a`。ベースラインを記録したコミット | glzr-io/zebarの`5f90d341` | `ci/live-expectations/upstream.txt` |
| `all-fixes` | ShortArrow/glazewmの`vmtest/all-fixes`の`8a74577a`。upstreamに、ハーネスで見つけた修正をすべて加えたもの | ShortArrow/zebarの`vmtest/all-fixes`の`d44213c0`。glazewmと同じく修正をすべて加えたもの | `ci/live-expectations/all-fixes.txt`。glazewmを昇格して起動するときは`ci/live-expectations/all-fixes-administrator.txt` |

続いてliveのジョブが、ランナーでそれぞれの期待結果のファイルを渡して`ci/run-live.sh`を順に実行する。スクリプトは、liveシナリオを1本ずつ、1つのゲストで順番に実行する。`fails`と書いたシナリオは失敗しなければならず、`varies`と書いたシナリオは実行せず、ほかのシナリオは成功しなければならない。upstreamのglazewmかzebarにまだ残っているバグを再現するシナリオは、`upstream.txt`に`fails`と書き、`all-fixes`では成功する。こうして1回の実行で、バグとその修正の両方を示す。

liveのワークフローには`pull_request`のトリガーがないので、プルリクエストが持ち込んだコードでliveシナリオが動くことはない。ただし、これだけではプルリクエストをランナーから締め出せない。`pull_request`のイベントでは、GitHubはプルリクエスト自身のワークフローを実行するので、`runs-on: vmtest-kvm`のワークフローを加えたプルリクエストは、それをランナーで実行する。つまり、ワークフローが実行されるプルリクエストを出せる人は、`vmtest-kvm`でコードを実行できる。非公開のリポジトリならその共同作業者、公開のリポジトリなら、外部の貢献者に対する承認の設定が実行を許したフォークからのプルリクエストの作者である。

## セルフホストのランナー

ランナーは、docs/GUEST-IMAGE.jp.mdのDockerホスト`vmtest-kvm`で、その上にdockur/windowsのゲストを作ってある。ランナーはユーザー`runner`で動く。このユーザーは`docker`グループに属し、`sudo`は使えない。以下の手順は`vmtest`で実行する。

1. ビルドに要るものを入れる。`sudo apt-get install -y build-essential pkg-config curl`。
2. ユーザーを作る。`sudo useradd --create-home --shell /bin/bash --groups docker runner`と`sudo passwd -l runner`。
3. そのユーザーにRustを入れる。`runner`で`curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.97.1`。
4. ゲストのパスワードとSSHの鍵を、`runner`だけが読める`/home/runner/vmtest-secrets`に置く。guests/dockur-windowsの`.env`と、guests/dockur-windows/.secretsにある秘密鍵を`id_ed25519`として置く。liveのジョブは両方を自分のチェックアウトにコピーし、終わるときに消す。
5. https://github.com/actions/runner/releases からランナーを`/home/runner/actions-runner`にダウンロードし、リリースノートのSHA-256と照合し、`sudo /home/runner/actions-runner/bin/installdependencies.sh`を実行する。
6. リポジトリのSettings → Actions → Runners → New self-hosted runnerで得た登録トークンで、`runner`として`/home/runner/actions-runner`で登録する。`./config.sh --unattended --url https://github.com/ShortArrow/glazewm-vmtest --token <トークン> --name vmtest-kvm --labels vmtest-kvm`。
7. `runner`で動くサービスとして入れる。`sudo sh -c 'cd /home/runner/actions-runner && ./svc.sh install runner && ./svc.sh start'`。`vmtest`は`/home/runner`に入れないので、`cd`も`sudo`の中で実行する。

ランナーはGitHubへ外向きのHTTPSでつなぎ、どのポートでも待ち受けない。
