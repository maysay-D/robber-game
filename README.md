# robber-game

## 起動
- `cargo run`

## タイトル画面
- 弱いCPU / 中程度のCPU / 強いCPU / プレイヤー対戦 を選択

## CPUモデル
- `src/rl_agent/` の `エージェント名_盤面サイズ_学習回数.pkl` を使用
- 弱いCPU: 学習回数が最少のモデル
- 中程度のCPU: 中央のモデル
- 強いCPU: 学習回数が最多のモデル

## 学習済みモデルの保存
- `src/rl_agent/train_offline.py` が `AlphaStomper_4x1_200.pkl` のような形式で保存

## 対人戦サーバー
- `cargo run --bin robber-server -- <width> <height>`
- 例: `cargo run --bin robber-server -- 4 1`

## Web配信について
- ブラウザで動かす場合は、Rust クライアントを `wasm32-unknown-unknown` にビルドして静的配信します
- その場合も対人戦は `robber-server` を別プロセスで起動します
- CPU 戦は現状ローカルの Python エージェントを呼ぶため、ブラウザ単体ではなくサーバー側へ移す構成が必要です
