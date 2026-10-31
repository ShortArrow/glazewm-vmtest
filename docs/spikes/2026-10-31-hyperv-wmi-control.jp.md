# スパイク: WMIによるHyper-Vゲストの操作

- 日付: 2026-10-31
- スパイク: docs/ROADMAP.jp.mdの「WMIによるHyper-Vの操作」
- ホスト: Windows 11 Pro ビルド26300のHyper-V。管理者としてSSHで入り、Windows PowerShell 5.1のCIMコマンドレットで実行した
- ゲスト: docs/GUEST-IMAGE.jp.mdのチェックポイントを持つ仮想マシン`vmtest-win11`。チェックポイントはすべて、ゲストを止めて取った標準チェックポイント

## 方法

すべての操作で`root\virtualization\v2`のクラスだけを使い、Hyper-VのPowerShellモジュールは読み込まなかった。メソッドが4096を返したときはジョブが始まっているので、そのジョブの`Msvm_ConcreteJob`を`JobState`が7（完了）以上になるまで調べた。

| 操作 | クラスとメソッド |
|---|---|
| ゲストを探す | `ElementName`がゲスト名の`Msvm_ComputerSystem` |
| チェックポイントを探す | `Msvm_SnapshotOfVirtualSystem`でゲストに関連づいた`Msvm_VirtualSystemSettingData`を、`ElementName`で選ぶ |
| チェックポイントに戻す | `Msvm_VirtualSystemSnapshotService.ApplySnapshot(Snapshot)` |
| チェックポイントを作る／消す | `Msvm_VirtualSystemSnapshotService.CreateSnapshot(AffectedSystem, SnapshotType=2)`／`DestroySnapshot(AffectedSnapshot)` |
| 起動／電源断 | `Msvm_ComputerSystem.RequestStateChange(RequestedState=2／3)`のあと、`EnabledState`が2／3になるまで調べる |
| ゲストOSのシャットダウン | ゲストに関連づいた`Msvm_ShutdownComponent.InitiateShutdown(Force=true, Reason)`のあと、`EnabledState`が3になるまで調べる |
| 生存確認 | ゲストに関連づいた`Msvm_HeartbeatComponent.OperationalStatus` |

使えるようになるまでの時間は、ゲストと同じネットワークにある別のWindows 11マシンから測った。起動を要求してから、`ssh fixture@<ゲスト>`が応答するまでと、セッション1で`explorer.exe`が動くまでである。

## 観察

| 操作 | 観察 |
|---|---|
| ゲストが止まった状態で`daemon-task`に戻す | 0.3秒。ゲストは止まったまま |
| 起動 | `EnabledState`が2になるまで0.5秒。ハートビートは5秒後にOK |
| `daemon-task`に戻してから起動 | 2回とも、11秒後にSSHが応答し、13秒後にセッション1で`explorer.exe`が動いた |
| 動いているゲストのチェックポイントを作る | 2.0秒。`vmtest-win11 - (<日付> - <時刻>)`という名前の`Microsoft:Hyper-V:Snapshot:Realized`ができる |
| そのチェックポイントを消す | 0.2秒 |
| ゲストOSのシャットダウン | `EnabledState`が3になるまで5〜10秒 |
| 電源断 | 0.3秒 |
| `-Association`を指定せずにゲストから関連をたどる | ほかの関連経由で、同じチェックポイントが複数回返った。その結果を`ApplySnapshot`に渡すと`Unable to cast object of type 'System.Object[]'`で失敗した |

## 設計への影響

- 起動、停止、シャットダウン、チェックポイントへの復元・作成・削除はすべてWMIでできる。Hyper-VのPowerShellモジュールが要る操作はない。
- ハートビートがOKでも、ゲストが使えるとは限らない。SSHとログオン済みのセッションは、その6〜8秒後だった。使えるかどうかはゲスト自身を通して確かめる必要がある。

## 測っていないこと

- Hyper-VのホストでSSH経由で呼ぶ代わりに、別のマシンからWMIを呼ぶこと（DCOMやWinRM）。
- どの種類のチェックポイントを使うか、また復元がゲストの時計と画面の状態に何をするか。これはスナップショットの種類のスパイクで扱う。
