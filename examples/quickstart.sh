#!/bin/bash
set -e
# rope 4 動詞クイックスタート
#
# 前提: `cargo install --path .` (ソースから) でインストール済み
#       ("rope" は crates.io で既に別プロジェクトに使われているため
#        cargo install rope では入手不可、README 参照)
#
# 注意: 現状これらのコマンドは実 P2P 通信・実 GPU 推論を伴わない。
# 鍵生成などのロジックは本物だが、ピア発見・握手・推論実行は
# 状態機械としては動くものの実 I/O が未配線 (詳細: SECURITY.md, docs/ASSESSMENT.md)

# 1. 初回起動 — 60 秒 wow moment
#    鍵生成 → ピア発見 → TEE 検証 → haiku 表示
rope

# 2. ピア発見 (LAN のみ、デフォルト)
rope pair

# 3. 推論実行 (ピアの GPU を借りて推論)
rope run llama-3.2-1b-instruct \
    --prompt "Explain GPU sharing in one sentence." \
    --privacy tee-only \
    --budget 100

# 4. GPU 貸し出し (1 sat/秒、最大 60 分)
rope earn --rate 1 --max-minutes 60
