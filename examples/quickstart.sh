#!/bin/bash
set -e
# rope 4 動詞クイックスタート
#
# 前提: cargo install rope 済み

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
