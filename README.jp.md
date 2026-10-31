# glazewm-vmtest

[GlazeWM](https://github.com/glzr-io/glazewm)と[Zebar](https://github.com/glzr-io/zebar)が、ディスプレイ構成の変化にどう振る舞うかを仮想マシン上で再現して検証するためのテストハーネス。

確認する変化は次のとおり。

- モニターの追加と削除
- 解像度、スケーリング、配置、プライマリの変更
- RDPセッションの接続、切断、再接続
- VNCクライアントの接続と切断
- スリープからの復帰、ロックとアンロック

物理モニターや実機は使わない。Hyper-VとQEMU/KVM（dockur/windows）上のWindowsゲストで、同じシナリオを繰り返し実行する。macOSゲストにはApple SiliconのMacホストが要る。docs/ROADMAP.jp.mdを参照。

## 検証対象

| 対象 | 既定のチェックアウト先 | プラットフォーム |
|---|---|---|
| glazewm | `../glazewm` | Windows、macOS |
| zebar | `../zebar` | Windows |

ハーネスは検証対象をホストでビルドし、成果物だけをゲストに配置する。ゲスト内ではビルドしない。

## ドキュメント

- [docs/PRD.jp.md](docs/PRD.jp.md) — 目的、ユースケース、要件、非ゴール
- [docs/ARCHITECTURE.jp.md](docs/ARCHITECTURE.jp.md) — 構成、依存方向、設計意図
- [docs/ROADMAP.jp.md](docs/ROADMAP.jp.md) — 段階計画、検証項目、未決事項
- [docs/GUEST-IMAGE.jp.md](docs/GUEST-IMAGE.jp.md) — Windowsゲストとスナップショットの作り方
- [docs/ADR/](docs/ADR/) — 複数案から選んだ決定とその理由

どの`*.md`にも、日本語訳の`*.jp.md`が隣にある（英語版は[README.md](README.md)）。内容が食い違うときは英語版に従う。

## 現状

docs/ROADMAP.jp.mdの段階0を進めている。Hyper-Vのゲストかdockur/windowsのゲストに対して、シナリオが端から端まで動く。ゲストデーモンとglazewmをビルドし、そのテストの冒頭にある環境変数を設定してから、`cargo test -p vmtest-adapters --test live -- --ignored`で実行する。検証の結果はdocs/spikes/に、途中で見つけたglazewmの振る舞いはdocs/findings/にある。
