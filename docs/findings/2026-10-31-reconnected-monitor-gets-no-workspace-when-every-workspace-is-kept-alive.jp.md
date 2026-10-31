# 発見: すべてのワークスペースをkeep_aliveにすると、つなぎ直したモニターにワークスペースがなくなる

- 日付: 2026-10-31
- 状態: ../glazewmの`fix/no-displayed-workspace-on-monitor-reconnect`（`207f9b5a`）で修正し、`vmtest/all-fixes`（`f8dc1eba`）にマージした。upstreamには報告していない
- SUT: glazewm `5709ad0a`と、`b801904d`時点の`vmtest/all-fixes`
- ゲスト: Hyper-V上のdocs/GUEST-IMAGE.jp.mdのチェックポイント`daemon-task`。ディスプレイは、Hyper-Vの合成ディスプレイ（1024×768）とVirtual Display Driverのもの

## 再現

crates/adapters/tests/live.rsのシナリオ`every_monitor_gets_a_workspace_back_when_every_workspace_is_kept_alive`で再現する。

1. ワークスペース`0`〜`9`の10個をすべて`keep_alive: true`にしてglazewmを起動する。`0`〜`5`と`9`には`bind_to_monitor: 1`を付ける。ゲストのディスプレイは、合成ディスプレイと3台の仮想ディスプレイである。
2. 仮想ディスプレイを無効にしてから、もう一度有効にする。

## 観察

| SUT | ディスプレイが戻った後 |
|---|---|
| `5709ad0a` | 「Non-fatal error」のダイアログに「No workspace config available to activate workspace.」と出る。ダイアログが開いている間、glazewmはIPCの要求に応答しない |
| `b801904d`時点の`vmtest/all-fixes`（このダイアログでイベントループが止まらない） | glazewmは応答するが、3台の仮想ディスプレイのうち2台（x = 1824とx = 2624の800×600）にワークスペースがない |
| `f8dc1eba`時点の`vmtest/all-fixes` | どのディスプレイにも表示中のワークスペースが1つある。合成ディスプレイに`6`、1台目の仮想ディスプレイに`0`〜`5`と`9`、残りの2台に`7`と`8` |

## ソースから読める仕組み

ディスプレイが外れると、そのワークスペースは残ったディスプレイへ移るので、10個とも動いたままになる。ディスプレイが足されると、packages/wm/src/commands/monitor/add_monitor.rsの`move_bounded_workspaces_to_new_monitor`が、そのインデックスに結びついたワークスペースを戻す。それでもワークスペースのないディスプレイには、`activate_workspace(None, …)`でワークスペースを与えようとする。これはまだ動いていないワークスペース設定からワークスペースを作る処理で、そうした設定が残っていないので「No workspace config available to activate workspace.」で失敗し、ディスプレイはワークスペースのないままになる。後でそのモニターの表示中のワークスペースを調べるコマンドは、「No displayed workspace.」で失敗する。

修正では、動いていないワークスペース設定が残っていないとき、ほかのモニターが表示しておらず、そのモニターに結びついてもいないワークスペースを、新しいディスプレイへ移す。

## ユーザーへの影響

すべてのワークスペースを動いたままにしているユーザーは、ディスプレイを外してつなぎ直すと、glazewmが認識していてもワークスペースを表示しないモニターができる。そのモニターの表示中のワークスペースを調べるコマンドは、「No displayed workspace.」で失敗する。upstreamのglazewmでは、さらにエラーのダイアログを閉じるまでウィンドウマネージャーが止まる。

## 未確認

- upstreamが、ワークスペースを移すより作るほうを望むかどうか。
- 戻ったディスプレイにユーザーがどのワークスペースを期待するか。修正は、モニターを順に、各モニターのワークスペースを順に見て、最初に見つかったものを移す。
