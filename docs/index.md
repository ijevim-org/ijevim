---
layout: home

hero:
  name: ijevim
  text: 軽量 Vim ライク TUI エディタ
  tagline: Rust 製。Vim / Emacs キーマップ、マルチ言語プラグイン、git レビューモードをひとつのバイナリに。
  image:
    src: /logo.svg
    alt: ijevim
  actions:
    - theme: brand
      text: はじめる
      link: /keybindings
    - theme: alt
      text: GitHub
      link: https://github.com/ijevim-org/ijevim

features:
  - icon: ⌨️
    title: Vim / Emacs キーマップ
    details: Vim のモード・オペレータ・レジスタから Emacs の kill-ring / mark-region まで。起動時に切り替え可能。
  - icon: 🧩
    title: マルチ言語プラグイン
    details: Lua / Lisp / JavaScript / Rust / Nix のローダーを feature で選択。必要なランタイムだけビルドに含められます。
  - icon: 🎨
    title: 構文ハイライト
    details: syntect によるシンタックスハイライトを任意の syntax feature で有効化。
  - icon: 🔍
    title: レビューモード
    details: git diff を TUI 上で確認し、ハンク操作とコメントをその場で行えます。
---

## クイックスタート

```sh
git clone https://github.com/ijevim-org/ijevim
cd ijevim
cargo run -- README.md
```

Vim キーマップで起動するには:

```sh
cargo run -- vim README.md
```

利用可能なドキュメントはサイドバーから辿れます。まずは [キーバインド仕様](/keybindings) を読んでください。
