# 発見: 仮想モニターが戻るときにWindowsがタイル配置中のウィンドウを最小化し、glazewmが最小化のままにする

- 日付: 2026-10-31
- 状態: ../glazewmの`fix/restore-windows-minimized-by-display-change`（`d43f1d1c`）で修正し、`vmtest/all-fixes`（`2f4ff441`）にマージした。upstreamには報告していない
- SUT: glazewm `5709ad0a`。設定はサンプルから起動時と終了時のコマンドを除いたもの
- ゲスト: Hyper-V上のdocs/GUEST-IMAGE.jp.mdのチェックポイント`daemon-task`。ディスプレイはHyper-Vの合成ディスプレイ（1280×800）とVirtual Display Driver。Windows 11 ビルド26300

## 再現手順

crates/adapters/tests/live.rsのシナリオ`tiled_test_windows_stay_within_their_working_area_across_monitor_count_changes`で行う。

1. 合成ディスプレイと、その右の仮想モニター1台から始め、glazewmを起動する。
2. テスト用ウィンドウを3つ開く。ウィンドウは仮想モニター上のワークスペース2にタイル配置される。
3. 仮想モニターの数を3にする。すべての仮想モニターが外れ、ワークスペース2とそのウィンドウが合成ディスプレイに移った状態でglazewmが落ち着いてから、3台の仮想モニターが来る。

## 観測結果

7回中2回、手順3の後にテスト用ウィンドウ0が最小化されていた。OSはこのウィンドウを(-31993, -32000)に置き、glazewmは状態を`minimized`と報告した。残り2つのウィンドウがワークスペース全体にタイル配置された。後のステップも比較した1回では、シナリオの最後まで最小化のままだった。不変条件の違反はなく、違いはベースラインからの逸脱としてだけ現れた。

その1回のglazewmのログには、glazewmが最小化を指示した記録はない。仮想モニターが外れたとき、glazewmはワークスペース2を合成ディスプレイに移し、ウィンドウはタイル配置のままだった。ドライバーのデバイスを有効に戻すと、glazewmが3台のモニターを足す変更を処理する0.6秒前に、glazewmはテスト用ウィンドウ0について`Window minimized`と`Updating window state: Minimized`を記録した。

## 引き金

Windows 11は、`MonitorRemovalRecalcBehavior`（DWORD、`HKCU\Control Panel\Desktop`）が1でない限り、切断されたディスプレイのウィンドウを最小化する。この値はMicrosoftの文書にはない。名前と意味は、winaero.comの「Windows 11: Don't minimize windows when monitor is disconnected」（2026-10-31に読んだ）などのサードパーティの解説による。ゲストイメージはこの値を設定していないので、ゲストは最小化する。

この値を1にしたゲストの複製で同じシナリオを15回実行すると、すべてベースラインと一致し、glazewmは最小化を記録しなかった。7回中2回最小化するなら、15回続けて最小化しない確率は1%未満である。

## 修正

`fix/restore-windows-minimized-by-display-change`は、ディスプレイの変化の前後5秒以内の最小化をOSによるものとみなし、ウィンドウを前の状態に戻す。その5秒の間に利用者が自分で最小化した場合も元に戻る。

最初の版はディスプレイの変化の後の最小化だけを見ており、15回中4回でウィンドウが最小化のまま残った。それらの回では、glazewmはディスプレイの変化の0.07〜0.19秒前に最小化を知っていた。前後どちらの順序も見るようにした版では、Windowsの既定のままのゲストで15回実行し、ウィンドウが最小化されたのは2回だった。glazewmはその2回とも`Window minimized by a display change, restoring`を記録し、最小化のまま残ったウィンドウはなかった。

## 発見時に未解決だった点

- glazewmは、OSが報告する最小化をすべて利用者の操作として扱う。そのため、Windowsがディスプレイの変化のために最小化したウィンドウも、タイル配置から外したままにする。glazewmがこの2つを区別すべきか（例えば、ディスプレイの変化の最中にOSが最小化したウィンドウを元に戻す）は決まっていない。
- Windowsが、glazewmがウィンドウを残ったディスプレイに移した後、仮想モニターが戻るときになって最小化した理由と、それが一部の実行でだけ起きる理由。
