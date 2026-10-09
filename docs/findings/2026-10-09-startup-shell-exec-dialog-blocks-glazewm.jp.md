# 発見: 起動時の`shell-exec`が失敗すると、ダイアログを2つ閉じるまでglazewmが止まる

- 日付: 2026-10-09
- 状態: 候補。upstreamには報告していない
- SUT: glazewm `5709ad0a`。ビルド方法はdocs/spikes/2026-10-09-first-glazewm-run.jp.mdのとおり
- ゲスト: docs/GUEST-IMAGE.jp.mdのスナップショット`daemon-task`。zebarは入っていない

## 再現手順

1. `general.startup_commands`が`['shell-exec zebar']`であるglazewm自身の`resources/assets/sample-config.yaml`で、`zebar`が見つからないマシンでglazewmを起動する。
2. `glazewm-cli query monitors`を実行する。

## 観測結果

| 時点 | 画面 | `glazewm-cli query monitors` |
|---|---|---|
| 起動後 | シェルのダイアログ「Windows cannot find 'zebar'」 | 8秒以内に応答なし。別のセッションからも同じ |
| それをEnterで閉じた後 | glazewmのダイアログ「Non-fatal error — Shell exec failed for 'zebar' …（0x800704C7）」 | 8秒以内に応答なし |
| それも閉じた後 | ダイアログなし | 280ミリ秒で応答。待っていた問い合わせもすべて完了した |

どちらかのダイアログが開いているあいだ、glazewmはIPCの要求に一切応答しない。

## ソースから分かる仕組み

packages/wm-platform/src/dispatcher.rsの`shell_execute_ex`は、`ShellExecuteExW`を`fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC`で呼び、`SEE_MASK_FLAG_NO_UI`を付けていない。そのため、プログラムが見つからないと、シェルが呼び出し元のスレッドでモーダルダイアログを出す。2つ目のダイアログは、その結果のエラーをglazewm自身が報告したもの。コマンドを実行したスレッドはIPCを処理するスレッドでもあり、両方のダイアログがIPCを止めたことと整合する。デバッガーでは確かめていない。

## 利用者への影響

アンインストール済みのプログラムを起動する設定を使っている利用者は、2つのダイアログを見つけて閉じるまで、IPCに応答しない（同じスレッドがウィンドウのイベントも処理しているなら、ウィンドウも管理しない）ウィンドウマネージャーを使うことになる。

## 未確認のこと

- ダイアログが開いているあいだ、IPCだけでなくウィンドウ管理も止まるか。
- キーバインドからの`shell-exec`でも同じように止まるか。
- upstreamがこのダイアログを意図しているか。
