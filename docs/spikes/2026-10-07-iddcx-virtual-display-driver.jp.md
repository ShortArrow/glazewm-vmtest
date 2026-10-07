# 検証記録: dockur/windowsでのIddCx仮想ディスプレイドライバーによるモニター変更

- 日付: 2026-10-07
- 検証項目: docs/ROADMAP.jp.mdの「ゲスト内の仮想ディスプレイドライバー（IddCx）」
- ゲスト: docs/spikes/2026-10-07-interactive-session-dockur.jp.mdのゲストのスナップショット`uac-on`。元の画面はRed Hat VirtIO GPU DODコントローラーの1280×800の出力1つ。
- ドライバー: VirtualDrivers/Virtual-Display-Driverのリリース`25.7.23`、アセット`VirtualDisplayDriver-x86.Driver.Only.zip`（名前はx86だが中身はamd64用）。SHA-256は`e24210692b442b39af763536330ce78b423f19342b7a7792c26de3944e418b3a`で、GitHubが公開する値と一致。ライセンスはMIT。カタログはSignPath Foundationが署名し、発行元はGlobalSign GCC R45 CodeSigning CA 2020。ビルドの来歴証明は公開されていない。

## 方法

- セッション1に、glazewmが監視用ウィンドウを作るのと同じ形の隠しトップレベルウィンドウを置き、`WM_DISPLAYCHANGE`、`SPI_SETWORKAREA`付きの`WM_SETTINGCHANGE`、`DBT_DEVNODES_CHANGED`付きの`WM_DEVICECHANGE`を、その時点の`EnumDisplayMonitors`のモニター一覧と合わせて記録した。
- ドライバーはSSHから入れた。SetupAPIでハードウェアID`Root\MttVDD`のルート列挙デバイスを作り、`pnputil /add-driver /install`を実行した。
- モニター数は、名前付きパイプ`MTTVirtualDisplayPipe`に`SETDISPLAYCOUNT <n>`を書いて変えた。デバイスの無効化と有効化には`Disable-PnpDevice`/`Enable-PnpDevice`を使った。

## 観測結果

| 操作 | 結果 |
|---|---|
| ユーザー操作なしでのインストール | 署名者の証明書を`LocalMachine\TrustedPublisher`に登録するまで`0xE0000242`で失敗した |
| 最初のインストール | 1枚 → 2枚。3種類のメッセージがすべて届いた。新しいモニターは`DISPLAY2`、位置1280,0、800×600 |
| `SETDISPLAYCOUNT 3`、`2`、`1` | 変更のたびに仮想モニターが全部消え、約0.3〜0.5秒後に新しい枚数で作り直された。1回の変更でメッセージが10〜15個届き、その一部は元の1枚だけの時点で届いた。落ち着いた後の枚数は元の1枚＋n |
| `SETDISPLAYCOUNT 0` | ドライバーのソース（`if (monitorcount == 0) monitorcount = 1`）のとおり1として扱われた |
| デバイス名 | 作り直すたびに新しい名前になった。`DISPLAY3`〜`5`、次に`6`〜`7`、`8`、`9` |
| デバイスの無効化・有効化 | 仮想モニターがまとめて外れ、まとめて戻った。2枚 → 1枚 → 2枚、それぞれメッセージ約5個 |

## 設計への影響

- 仮想モニターの変化でも、glazewmが監視するメッセージは届く。dockurのゲストで、ホストの入力なしに起こせる。
- このドライバーが変えられるのはモニターの枚数で、個々のモニターではない。3枚のうち中央の1枚だけを外す（UC-1）は表せず、どの変更も途中で元の1枚だけのトポロジーを経由する。
- 基盤の元の画面は残る。全ディスプレイの消失（UC-6）には別の手段が要る。

## 測れなかったこと

- 仮想モニターの配置、解像度、プライマリの変更。
- glazewm自身の反応。glazewmは動かしていない。
