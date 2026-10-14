# 発見: 起動時の`shell-exec`が失敗すると、ダイアログを2つ閉じるまでglazewmが止まる

- 日付: 2026-10-09
- 状態: ../glazewmで修正済み。upstream向けに原因ごとのブランチ（`fix/shell-exec-without-ui`、`fix/non-fatal-error-dialog-off-event-loop`）があり、`vmtest/all-fixes`に両方が入っている。upstreamには報告していない。upstreamのissue glzr-io/glazewm#705は、2つ目のダイアログを停止ではなく警告として扱っている
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

packages/wm-platform/src/dispatcher.rsの`shell_execute_ex`は、`ShellExecuteExW`を`fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC`で呼び、`SEE_MASK_FLAG_NO_UI`を付けていない。そのため、プログラムが見つからないと、シェルが呼び出し元のスレッドでモーダルダイアログを出す。2つ目のダイアログは、その結果のエラーをglazewm自身が報告したもの。packages/wm/src/main.rsの`start_wm`は、非致命的なエラーをすべて`show_error_dialog`で表示する。これは`MB_SYSTEMMODAL`付きの`MessageBoxW`で、閉じられるまで戻らず、イベントループを動かすスレッドの上で呼ばれる。このループはIPCのメッセージとウィンドウのイベントも受け取るので、2つ目のダイアログは両方を止める。`shell-exec`に限らず、どのコマンドが失敗しても同じになる。

crates/adapters/tests/live_dockur.rsのシナリオ`glazewm_answers_ipc_after_a_startup_command_fails`は、`5709ad0a`では`glazewm_answers_ipc`の違反を報告し、`vmtest/all-fixes`では通る。このブランチは`SEE_MASK_FLAG_NO_UI`を付け、非致命的なエラーのダイアログを別のスレッドで表示する。

## 利用者への影響

インストールされていないプログラムを起動する設定を使っている利用者は、2つのダイアログを見つけて閉じるまで、IPCにもウィンドウのイベントにも応じないウィンドウマネージャーを使うことになる。glazewmはステータスバーの選択を利用者に任せており、サンプル設定はzebarを起動する。そのため、zebar以外のバーを使う人やバーを使わない人で、サンプルの起動コマンドを残している人はすべて該当する。

## 未確認のこと

- ダイアログが開いているあいだウィンドウ管理も止まることは、ソースから読み取っただけで、観測していない。
- キーバインドからの`shell-exec`でも同じように止まるか。
- upstreamがこのダイアログを意図しているか。
