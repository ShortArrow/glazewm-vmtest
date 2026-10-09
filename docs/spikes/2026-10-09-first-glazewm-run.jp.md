# 検証記録: dockur/windowsでのモニター数変化に対するglazewm

- 日付: 2026-10-09
- ゲスト: スナップショット`vdd`（docs/spikes/2026-10-07-iddcx-virtual-display-driver.jp.md）に、Microsoft Visual C++ 2015〜2022 x64再頒布可能パッケージを加えたもの。スナップショット`vcredist`として保存
- SUT: glazewm `5709ad0a`（upstreamの`main`）。ホストで`cargo +nightly build --release --locked -p wm -p wm-cli -p wm-watcher`（nightly 2026-02-17）でビルド。`ui_access`機能なし
- 設定: glazewm自身の`resources/assets/sample-config.yaml`から`startup_commands`と`shutdown_commands`を空にしたもの。`GLAZEWM_CONFIG_PATH`で渡した

## 観測結果

| 確かめたこと | 結果 |
|---|---|
| releaseビルドは素のゲストで起動するか | しない。終了コード`0xC0000135`（DLLが見つからない）。バイナリは`vcruntime140.dll`をインポートしている。glazewmのリリース用ワークフローも同じ方法でビルドしているので、公式のバイナリにも必要。再頒布可能パッケージを入れると起動した |
| glazewmをユーザーの整合性レベルでセッション1に起動できるか | できる。`RunLevel Limited`の対話スケジュールタスクで起動し、`glazewm`と`glazewm-watcher`がセッション1で動いた |
| ハーネスはSSHからglazewmに問い合わせられるか | できる。IPCサーバーはTCPで待ち受けているので、セッション0から`glazewm-cli query monitors`が応答した |

モニター数のシナリオ。各ステップは操作の6秒後に観測した。

| ステップ | glazewmのモニター（ワークスペース） | OSのモニター数 |
|---|---|---|
| 開始時 | `DISPLAY1`（1）、`DISPLAY2`（2） | 2 |
| 仮想3枚 | `DISPLAY1`（1）、`DISPLAY3`（2）、`DISPLAY4`（3）、`DISPLAY5`（4） | 4 |
| 仮想1枚 | `DISPLAY1`（1）、`DISPLAY6`（2） | 2 |
| デバイス無効化 | `DISPLAY1`（1） | 1 |
| デバイス有効化 | `DISPLAY1`（1）、`DISPLAY7`（2） | 2 |

どのステップの後も、glazewmのモニター数はOSと一致した。外れたモニターの空のワークスペースは消えた。ウィンドウは開いていない。

## 測れなかったこと

- 外れるモニターにウィンドウがある場合。
- 6秒後が落ち着いた状態だったか。落ち着いた状態の判定は使っていない。
- 公式ビルドの`ui_access`機能。効かせるには署名と、保護された場所へのインストールが要る。
